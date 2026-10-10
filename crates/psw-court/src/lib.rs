//! psw-008 — Court: adjudication of claims.
//! Claims arrive with evidence items, each tagged supports/refutes/neutral.
//! The rule is mechanical and published:
//!   - any refuting evidence            -> REFUTED
//!   - >=2 independent supporting items -> SUPPORTED
//!   - otherwise                        -> INCONCLUSIVE
//! Verdicts are recorded with the evidence snapshot; re-ruling appends a
//! new verdict, never rewrites history.
mod organelle;
pub use organelle::*;
use pw_domain::{AuthorityTier, Claim, EpistemicStatus, Verdict};
use pw_manifest::{Contract, OrganelleManifest, ReceiptPolicy, Transport, SCHEMA_VERSION};
use pw_organelles::{Organelle, OrganelleError};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EvidenceStance {
    Supports,
    Refutes,
    Neutral,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Evidence {
    pub id: String,
    pub stance: EvidenceStance,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ruling {
    pub claim_id: String,
    pub verdict: Verdict,
    pub supports: usize,
    pub refutes: usize,
    pub evidence_ids: Vec<String>,
    pub rationale: String,
}

pub struct Court {
    claims: Mutex<HashMap<String, (Claim, Vec<Evidence>)>>,
    rulings: Mutex<Vec<Ruling>>,
}

impl Court {
    pub fn new() -> Self {
        Court { claims: Mutex::new(HashMap::new()), rulings: Mutex::new(Vec::new()) }
    }

    pub fn file(&self, statement: &str, evidence: Vec<Evidence>) -> String {
        let claim = Claim::new(statement, evidence.iter().map(|e| e.id.clone()).collect());
        let id = claim.id.clone();
        self.claims.lock().unwrap().insert(id.clone(), (claim, evidence));
        id
    }

    /// The published mechanical rule.
    pub fn rule(claim_id: &str, evidence: &[Evidence]) -> Ruling {
        let supports = evidence.iter().filter(|e| e.stance == EvidenceStance::Supports).count();
        let refutes = evidence.iter().filter(|e| e.stance == EvidenceStance::Refutes).count();
        let (verdict, rationale) = if refutes > 0 {
            (Verdict::Refuted, format!("{refutes} refuting item(s) present: refutation dominates"))
        } else if supports >= 2 {
            (Verdict::Supported, format!("{supports} independent supporting items, none refuting"))
        } else {
            (Verdict::Inconclusive, format!("only {supports} supporting item(s): below threshold of 2"))
        };
        Ruling {
            claim_id: claim_id.into(),
            verdict,
            supports,
            refutes,
            evidence_ids: evidence.iter().map(|e| e.id.clone()).collect(),
            rationale,
        }
    }

    pub fn adjudicate(&self, claim_id: &str) -> Result<Ruling, String> {
        let claims = self.claims.lock().unwrap();
        let (claim, evidence) = claims.get(claim_id).ok_or_else(|| format!("unknown claim {claim_id}"))?;
        let ruling = Self::rule(&claim.id, evidence);
        self.rulings.lock().unwrap().push(ruling.clone());
        Ok(ruling)
    }

    pub fn rulings_for(&self, claim_id: &str) -> Vec<Ruling> {
        self.rulings.lock().unwrap().iter().filter(|r| r.claim_id == claim_id).cloned().collect()
    }
}

impl Default for Court {
    fn default() -> Self {
        Court::new()
    }
}

fn manifest() -> OrganelleManifest {
    OrganelleManifest {
        schema: SCHEMA_VERSION.into(),
        name: "psw-court".into(),
        version: "0.1.0".into(),
        description: "Claim adjudication under a published mechanical rule".into(),
        contracts: vec![
            Contract { name: "file".into(), input_schema: "{\"statement\":str,\"evidence\":[...]}".into(), output_schema: "{\"claim_id\":str}".into(), idempotent: false },
            Contract { name: "adjudicate".into(), input_schema: "{\"claim_id\":str}".into(), output_schema: "Ruling".into(), idempotent: false },
            Contract { name: "rulings".into(), input_schema: "{\"claim_id\":str}".into(), output_schema: "[Ruling]".into(), idempotent: true },
        ],
        authority: AuthorityTier::Operator,
        transports: vec![Transport::InProcess],
        receipts: ReceiptPolicy::Always,
        entrypoint: "psw_court::Court".into(),
    }
}

fn parse_evidence(v: &Value) -> Result<Vec<Evidence>, OrganelleError> {
    let arr = v.as_array().cloned().unwrap_or_default();
    arr.into_iter()
        .map(|e| serde_json::from_value(e).map_err(|e| OrganelleError::BadInput(e.to_string())))
        .collect()
}

impl Organelle for Court {
    fn name(&self) -> &'static str {
        "psw-court"
    }
    fn manifest(&self) -> OrganelleManifest {
        manifest()
    }
    fn call(&self, contract: &str, input: Value) -> Result<Value, OrganelleError> {
        match contract {
            "file" => {
                let statement = input.get("statement").and_then(|v| v.as_str()).ok_or_else(|| OrganelleError::BadInput("statement".into()))?;
                let evidence = parse_evidence(input.get("evidence").unwrap_or(&Value::Null))?;
                let id = self.file(statement, evidence);
                Ok(serde_json::json!({"claim_id": id}))
            }
            "adjudicate" => {
                let claim_id = input.get("claim_id").and_then(|v| v.as_str()).ok_or_else(|| OrganelleError::BadInput("claim_id".into()))?;
                let ruling = self.adjudicate(claim_id).map_err(OrganelleError::Execution)?;
                Ok(serde_json::to_value(ruling).unwrap())
            }
            "rulings" => {
                let claim_id = input.get("claim_id").and_then(|v| v.as_str()).ok_or_else(|| OrganelleError::BadInput("claim_id".into()))?;
                Ok(serde_json::to_value(self.rulings_for(claim_id)).unwrap())
            }
            _ => Err(OrganelleError::UnknownContract(contract.into(), self.name().into())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(id: &str, stance: EvidenceStance) -> Evidence {
        Evidence { id: id.into(), stance, summary: "s".into() }
    }

    #[test]
    fn two_supports_supported() {
        let r = Court::rule("c", &[ev("a", EvidenceStance::Supports), ev("b", EvidenceStance::Supports)]);
        assert_eq!(r.verdict, Verdict::Supported);
    }

    #[test]
    fn one_support_inconclusive() {
        let r = Court::rule("c", &[ev("a", EvidenceStance::Supports)]);
        assert_eq!(r.verdict, Verdict::Inconclusive);
    }

    #[test]
    fn refutation_dominates() {
        let r = Court::rule(
            "c",
            &[ev("a", EvidenceStance::Supports), ev("b", EvidenceStance::Supports), ev("c", EvidenceStance::Refutes)],
        );
        assert_eq!(r.verdict, Verdict::Refuted);
    }

    #[test]
    fn no_evidence_inconclusive() {
        let r = Court::rule("c", &[]);
        assert_eq!(r.verdict, Verdict::Inconclusive);
    }

    #[test]
    fn neutral_does_not_count() {
        let r = Court::rule("c", &[ev("a", EvidenceStance::Neutral), ev("b", EvidenceStance::Neutral)]);
        assert_eq!(r.verdict, Verdict::Inconclusive);
    }

    #[test]
    fn adjudicate_unknown_claim_errors() {
        let c = Court::new();
        assert!(c.adjudicate("ghost").is_err());
    }

    #[test]
    fn rulings_append_never_rewrite() {
        let c = Court::new();
        let id = c.file("s", vec![ev("a", EvidenceStance::Supports)]);
        c.adjudicate(&id).unwrap();
        c.adjudicate(&id).unwrap();
        assert_eq!(c.rulings_for(&id).len(), 2);
    }

    #[test]
    fn organelle_file_adjudicate() {
        let c = Court::new();
        let out = c.call("file", serde_json::json!({
            "statement": "x",
            "evidence": [
                {"id": "e1", "stance": "supports", "summary": "s"},
                {"id": "e2", "stance": "supports", "summary": "s"}
            ]
        })).unwrap();
        let claim_id = out["claim_id"].as_str().unwrap();
        let ruling = c.call("adjudicate", serde_json::json!({"claim_id": claim_id})).unwrap();
        assert_eq!(ruling["verdict"], "SUPPORTED");
    }

    #[test]
    fn claim_status_trinary() {
        let claim = Claim::new("x", vec![]);
        assert_eq!(claim.status, EpistemicStatus::Unknown);
    }
}
