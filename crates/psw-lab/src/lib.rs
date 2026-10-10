//! psw-002 — Lab: the experiment sandbox.
//! Mechanical pre-registration gate: no experiment runs without a committed
//! pre-registration (hypothesis + method + falsifier). Runs are recorded
//! against their pre-reg; results never mutate the pre-reg itself.
mod organelle;
pub use organelle::*;
use pw_domain::AuthorityTier;
use pw_manifest::{Contract, OrganelleManifest, ReceiptPolicy, Transport, SCHEMA_VERSION};
use pw_organelles::{Organelle, OrganelleError};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreReg {
    pub exp_id: String,
    pub hypothesis: String,
    pub method: String,
    pub falsifier: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunRecord {
    pub exp_id: String,
    pub run_no: u64,
    pub result: String,
    pub notes: String,
}

#[derive(Debug, thiserror::Error)]
pub enum LabError {
    #[error("pre-reg {0:?} missing hypothesis/method/falsifier")]
    IncompletePreReg(String),
    #[error("no pre-registration for {0:?}: run refused")]
    NoPreReg(String),
    #[error("duplicate pre-registration {0:?}")]
    Duplicate(String),
}

pub struct Lab {
    preregs: Mutex<HashMap<String, PreReg>>,
    runs: Mutex<Vec<RunRecord>>,
}

impl Lab {
    pub fn new() -> Self {
        Lab { preregs: Mutex::new(HashMap::new()), runs: Mutex::new(Vec::new()) }
    }

    pub fn preregister(&self, p: PreReg) -> Result<(), LabError> {
        if p.hypothesis.trim().is_empty() || p.method.trim().is_empty() || p.falsifier.trim().is_empty() {
            return Err(LabError::IncompletePreReg(p.exp_id));
        }
        let mut pr = self.preregs.lock().unwrap();
        if pr.contains_key(&p.exp_id) {
            return Err(LabError::Duplicate(p.exp_id));
        }
        pr.insert(p.exp_id.clone(), p);
        Ok(())
    }

    /// Fail-closed: refuses to run anything without a pre-registration.
    pub fn run(&self, exp_id: &str, result: &str, notes: &str) -> Result<RunRecord, LabError> {
        if !self.preregs.lock().unwrap().contains_key(exp_id) {
            return Err(LabError::NoPreReg(exp_id.into()));
        }
        let mut runs = self.runs.lock().unwrap();
        let run_no = runs.iter().filter(|r| r.exp_id == exp_id).count() as u64 + 1;
        let rec = RunRecord { exp_id: exp_id.into(), run_no, result: result.into(), notes: notes.into() };
        runs.push(rec.clone());
        Ok(rec)
    }

    pub fn runs_for(&self, exp_id: &str) -> Vec<RunRecord> {
        self.runs.lock().unwrap().iter().filter(|r| r.exp_id == exp_id).cloned().collect()
    }
}

impl Default for Lab {
    fn default() -> Self {
        Lab::new()
    }
}

fn manifest() -> OrganelleManifest {
    OrganelleManifest {
        schema: SCHEMA_VERSION.into(),
        name: "psw-lab".into(),
        version: "0.1.0".into(),
        description: "Experiment sandbox with mechanical pre-registration gate".into(),
        contracts: vec![
            Contract { name: "preregister".into(), input_schema: "PreReg".into(), output_schema: "{\"ok\":bool}".into(), idempotent: false },
            Contract { name: "run".into(), input_schema: "exp_id+result".into(), output_schema: "RunRecord".into(), idempotent: false },
            Contract { name: "runs".into(), input_schema: "{\"exp_id\":str}".into(), output_schema: "[RunRecord]".into(), idempotent: true },
        ],
        authority: AuthorityTier::Operator,
        transports: vec![Transport::InProcess],
        receipts: ReceiptPolicy::Always,
        entrypoint: "psw_lab::Lab".into(),
    }
}

impl Organelle for Lab {
    fn name(&self) -> &'static str {
        "psw-lab"
    }
    fn manifest(&self) -> OrganelleManifest {
        manifest()
    }
    fn call(&self, contract: &str, input: Value) -> Result<Value, OrganelleError> {
        match contract {
            "preregister" => {
                let p: PreReg = serde_json::from_value(input).map_err(|e| OrganelleError::BadInput(e.to_string()))?;
                self.preregister(p).map_err(|e| OrganelleError::Execution(e.to_string()))?;
                Ok(serde_json::json!({"ok": true}))
            }
            "run" => {
                let exp_id = input.get("exp_id").and_then(|v| v.as_str()).ok_or_else(|| OrganelleError::BadInput("exp_id".into()))?;
                let result = input.get("result").and_then(|v| v.as_str()).unwrap_or("");
                let notes = input.get("notes").and_then(|v| v.as_str()).unwrap_or("");
                let rec = self.run(exp_id, result, notes).map_err(|e| OrganelleError::Execution(e.to_string()))?;
                Ok(serde_json::to_value(rec).unwrap())
            }
            "runs" => {
                let exp_id = input.get("exp_id").and_then(|v| v.as_str()).ok_or_else(|| OrganelleError::BadInput("exp_id".into()))?;
                Ok(serde_json::to_value(self.runs_for(exp_id)).unwrap())
            }
            _ => Err(OrganelleError::UnknownContract(contract.into(), self.name().into())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn preg(id: &str) -> PreReg {
        PreReg { exp_id: id.into(), hypothesis: "h".into(), method: "m".into(), falsifier: "f".into() }
    }

    #[test]
    fn preregister_then_run() {
        let lab = Lab::new();
        lab.preregister(preg("e1")).unwrap();
        let r = lab.run("e1", "ok", "").unwrap();
        assert_eq!(r.run_no, 1);
    }

    #[test]
    fn run_without_prereg_refused() {
        let lab = Lab::new();
        assert!(matches!(lab.run("ghost", "x", ""), Err(LabError::NoPreReg(_))));
    }

    #[test]
    fn incomplete_prereg_refused() {
        let lab = Lab::new();
        let mut p = preg("e2");
        p.falsifier = "".into();
        assert!(matches!(lab.preregister(p), Err(LabError::IncompletePreReg(_))));
    }

    #[test]
    fn duplicate_prereg_refused() {
        let lab = Lab::new();
        lab.preregister(preg("e3")).unwrap();
        assert!(matches!(lab.preregister(preg("e3")), Err(LabError::Duplicate(_))));
    }

    #[test]
    fn run_numbers_increment() {
        let lab = Lab::new();
        lab.preregister(preg("e4")).unwrap();
        lab.run("e4", "r1", "").unwrap();
        let r2 = lab.run("e4", "r2", "").unwrap();
        assert_eq!(r2.run_no, 2);
        assert_eq!(lab.runs_for("e4").len(), 2);
    }

    #[test]
    fn organelle_preregister_contract() {
        let lab = Lab::new();
        let out = lab.call("preregister", serde_json::json!({"exp_id":"x","hypothesis":"h","method":"m","falsifier":"f"})).unwrap();
        assert_eq!(out["ok"], true);
    }

    #[test]
    fn organelle_run_without_prereg_errors() {
        let lab = Lab::new();
        assert!(lab.call("run", serde_json::json!({"exp_id":"ghost","result":"r"})).is_err());
    }

    #[test]
    fn organelle_runs_lists() {
        let lab = Lab::new();
        lab.call("preregister", serde_json::json!({"exp_id":"y","hypothesis":"h","method":"m","falsifier":"f"})).unwrap();
        lab.call("run", serde_json::json!({"exp_id":"y","result":"r"})).unwrap();
        let out = lab.call("runs", serde_json::json!({"exp_id":"y"})).unwrap();
        assert_eq!(out.as_array().unwrap().len(), 1);
    }
}
