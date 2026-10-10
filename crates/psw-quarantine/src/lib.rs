//! psw-009 — Quarantine: containment for untrusted payloads.
//! Nothing untrusted executes here. `inspect` scores a payload for risk
//! signals; `contain` admits it only into a sealed report, never into
//! execution. Fail-closed: high-risk payloads are refused outright, and
//! there is deliberately NO `execute` contract.
mod organelle;
pub use organelle::*;
use pw_domain::AuthorityTier;
use pw_manifest::{Contract, OrganelleManifest, ReceiptPolicy, Transport, SCHEMA_VERSION};
use pw_organelles::{Organelle, OrganelleError};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InspectionReport {
    pub risk_score: u8,
    pub signals: Vec<String>,
    pub verdict: ContainmentVerdict,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ContainmentVerdict {
    Clean,
    Suspicious,
    Refused,
}

/// Risk signals inspected in a JSON payload (recursive).
fn scan(v: &Value, signals: &mut Vec<String>, score: &mut u8) {
    match v {
        Value::Object(m) => {
            for (k, val) in m {
                let kl = k.to_lowercase();
                // Prototype-pollution primitives: immediate refusal. These
                // corrupt the object graph itself, not just data.
                for pat in ["__proto__", "constructor", "prototype"] {
                    if kl.contains(pat) {
                        signals.push(format!("key:{k}"));
                        *score = score.saturating_add(50);
                    }
                }
                for pat in ["eval", "exec", "spawn", "shell", "require("] {
                    if kl.contains(pat) {
                        signals.push(format!("key:{k}"));
                        *score = score.saturating_add(30);
                    }
                }
                scan(val, signals, score);
            }
        }
        Value::Array(a) => {
            for x in a {
                scan(x, signals, score);
            }
        }
        Value::String(s) => {
            let sl = s.to_lowercase();
            for pat in ["<script", "javascript:", "eval(", "child_process", "os.system", "curl | sh", "rm -rf /"] {
                if sl.contains(pat) {
                    signals.push(format!("string-pattern:{pat}"));
                    *score = score.saturating_add(25);
                }
            }
            if s.len() > 100_000 {
                signals.push("oversize-string".into());
                *score = score.saturating_add(10);
            }
        }
        _ => {}
    }
}

pub struct Quarantine;

impl Quarantine {
    pub fn inspect(&self, payload: &Value) -> InspectionReport {
        let mut signals = Vec::new();
        let mut score: u8 = 0;
        scan(payload, &mut signals, &mut score);
        signals.sort();
        signals.dedup();
        let verdict = if score >= 50 {
            ContainmentVerdict::Refused
        } else if score >= 20 {
            ContainmentVerdict::Suspicious
        } else {
            ContainmentVerdict::Clean
        };
        InspectionReport { risk_score: score.min(100), signals, verdict }
    }

    /// Admit a payload into containment. Refused payloads never get a
    /// container id — there is no path from here to execution.
    pub fn contain(&self, payload: &Value) -> Result<String, InspectionReport> {
        let report = self.inspect(payload);
        if report.verdict == ContainmentVerdict::Refused {
            return Err(report);
        }
        let digest = pw_domain::hash_hex(&payload.to_string());
        Ok(format!("qz-{}", &digest[..16]))
    }
}

fn manifest() -> OrganelleManifest {
    OrganelleManifest {
        schema: SCHEMA_VERSION.into(),
        name: "psw-quarantine".into(),
        version: "0.1.0".into(),
        description: "Containment: inspect untrusted payloads, refuse the dangerous ones, execute nothing".into(),
        contracts: vec![
            Contract { name: "inspect".into(), input_schema: "{\"payload\":json}".into(), output_schema: "InspectionReport".into(), idempotent: true },
            Contract { name: "contain".into(), input_schema: "{\"payload\":json}".into(), output_schema: "{\"container_id\":str}".into(), idempotent: false },
        ],
        authority: AuthorityTier::Operator,
        transports: vec![Transport::InProcess],
        receipts: ReceiptPolicy::Always,
        entrypoint: "psw_quarantine::Quarantine".into(),
    }
}

impl Organelle for Quarantine {
    fn name(&self) -> &'static str {
        "psw-quarantine"
    }
    fn manifest(&self) -> OrganelleManifest {
        manifest()
    }
    fn call(&self, contract: &str, input: Value) -> Result<Value, OrganelleError> {
        match contract {
            "inspect" => {
                let payload = input.get("payload").cloned().unwrap_or(Value::Null);
                Ok(serde_json::to_value(self.inspect(&payload)).unwrap())
            }
            "contain" => {
                let payload = input.get("payload").cloned().unwrap_or(Value::Null);
                match self.contain(&payload) {
                    Ok(id) => Ok(serde_json::json!({"container_id": id})),
                    Err(report) => Err(OrganelleError::Execution(format!(
                        "refused: {:?} signals={:?}",
                        report.verdict, report.signals
                    ))),
                }
            }
            _ => Err(OrganelleError::UnknownContract(contract.into(), self.name().into())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_payload_passes() {
        let q = Quarantine;
        let r = q.inspect(&serde_json::json!({"name": "widget", "count": 3}));
        assert_eq!(r.verdict, ContainmentVerdict::Clean);
        assert_eq!(r.risk_score, 0);
    }

    #[test]
    fn proto_pollution_flagged() {
        let q = Quarantine;
        let r = q.inspect(&serde_json::json!({"__proto__": {"x": 1}}));
        assert!(r.signals.iter().any(|s| s.contains("__proto__")));
        assert!(r.risk_score >= 20);
    }

    #[test]
    fn script_injection_refused() {
        let q = Quarantine;
        let r = q.inspect(&serde_json::json!({"note": "<script>alert(1)</script>", "cmd": "eval(1)"}));
        assert_eq!(r.verdict, ContainmentVerdict::Refused);
    }

    #[test]
    fn nested_signals_found() {
        let q = Quarantine;
        let r = q.inspect(&serde_json::json!({"a": {"b": [{"c": "javascript:alert(1)"}]}}));
        assert!(!r.signals.is_empty());
    }

    #[test]
    fn contain_clean_returns_id() {
        let q = Quarantine;
        let id = q.contain(&serde_json::json!({"ok": true})).unwrap();
        assert!(id.starts_with("qz-"));
    }

    #[test]
    fn contain_refused_has_no_id() {
        let q = Quarantine;
        let evil = serde_json::json!({"x": "rm -rf /", "y": "<script>"});
        assert!(q.contain(&evil).is_err());
    }

    #[test]
    fn no_execute_contract_declared() {
        let q = Quarantine;
        let m = q.manifest();
        assert!(!m.has_contract("execute"));
        assert!(q.call("execute", Value::Null).is_err());
    }

    #[test]
    fn organelle_inspect_contract() {
        let q = Quarantine;
        let out = q.call("inspect", serde_json::json!({"payload": {"a": 1}})).unwrap();
        assert_eq!(out["verdict"], "CLEAN");
    }
}
