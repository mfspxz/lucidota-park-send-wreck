//! psw-010 — Operator Cockpit: the command surface.
//! A release is a named sequence of stages. The cockpit moves stages
//! through PRE -> RUNNING -> DONE | FAILED, recording each transition.
//! A failed stage halts the release (fail-closed: no later stage runs).
use pw_domain::AuthorityTier;
use pw_manifest::{Contract, OrganelleManifest, ReceiptPolicy, Transport, SCHEMA_VERSION};
use pw_organelles::{Organelle, OrganelleError};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum StageState {
    Pre,
    Running,
    Done,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stage {
    pub name: String,
    pub state: StageState,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Release {
    pub id: String,
    pub name: String,
    pub stages: Vec<Stage>,
    pub halted: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum CockpitError {
    #[error("unknown release {0:?}")]
    UnknownRelease(String),
    #[error("release {0:?} halted after failure; stage {1:?} not started")]
    Halted(String, String),
    #[error("no stages")]
    NoStages,
    #[error("stage {0:?} not in release")]
    UnknownStage(String),
    #[error("stage {0:?} already terminal")]
    Terminal(String),
}

pub struct Cockpit {
    releases: Mutex<HashMap<String, Release>>,
}

impl Cockpit {
    pub fn new() -> Self {
        Cockpit { releases: Mutex::new(HashMap::new()) }
    }

    pub fn plan_release(&self, name: &str, stages: Vec<String>) -> Result<String, CockpitError> {
        if stages.is_empty() {
            return Err(CockpitError::NoStages);
        }
        let id = format!("rel-{}", &pw_domain::hash_hex(&format!("{name}{:?}", stages))[..12]);
        let release = Release {
            id: id.clone(),
            name: name.into(),
            stages: stages.into_iter().map(|s| Stage { name: s, state: StageState::Pre, note: String::new() }).collect(),
            halted: false,
        };
        self.releases.lock().unwrap().insert(id.clone(), release);
        Ok(id)
    }

    fn mutate(&self, id: &str, f: impl FnOnce(&mut Release) -> Result<(), CockpitError>) -> Result<(), CockpitError> {
        let mut rs = self.releases.lock().unwrap();
        let r = rs.get_mut(id).ok_or_else(|| CockpitError::UnknownRelease(id.into()))?;
        f(r)
    }

    pub fn start_stage(&self, id: &str, stage: &str) -> Result<(), CockpitError> {
        self.mutate(id, |r| {
            if r.halted {
                return Err(CockpitError::Halted(id.into(), stage.into()));
            }
            let s = r.stages.iter_mut().find(|s| s.name == stage).ok_or_else(|| CockpitError::UnknownStage(stage.into()))?;
            match s.state {
                StageState::Pre => {
                    s.state = StageState::Running;
                    Ok(())
                }
                _ => Err(CockpitError::Terminal(stage.into())),
            }
        })
    }

    pub fn finish_stage(&self, id: &str, stage: &str, ok: bool, note: &str) -> Result<(), CockpitError> {
        self.mutate(id, |r| {
            let s = r.stages.iter_mut().find(|s| s.name == stage).ok_or_else(|| CockpitError::UnknownStage(stage.into()))?;
            if s.state != StageState::Running {
                return Err(CockpitError::Terminal(stage.into()));
            }
            s.state = if ok { StageState::Done } else { StageState::Failed };
            s.note = note.into();
            if !ok {
                r.halted = true;
            }
            Ok(())
        })
    }

    pub fn status(&self, id: &str) -> Result<Release, CockpitError> {
        self.releases.lock().unwrap().get(id).cloned().ok_or_else(|| CockpitError::UnknownRelease(id.into()))
    }
}

impl Default for Cockpit {
    fn default() -> Self {
        Cockpit::new()
    }
}

fn manifest() -> OrganelleManifest {
    OrganelleManifest {
        schema: SCHEMA_VERSION.into(),
        name: "psw-cockpit".into(),
        version: "0.1.0".into(),
        description: "Operator command surface: plan and drive releases stage by stage".into(),
        contracts: vec![
            Contract { name: "plan".into(), input_schema: "{\"name\":str,\"stages\":[str]}".into(), output_schema: "{\"release_id\":str}".into(), idempotent: false },
            Contract { name: "start".into(), input_schema: "{\"release_id\":str,\"stage\":str}".into(), output_schema: "{\"ok\":bool}".into(), idempotent: false },
            Contract { name: "finish".into(), input_schema: "{\"release_id\":str,\"stage\":str,\"ok\":bool,\"note\":str}".into(), output_schema: "{\"ok\":bool}".into(), idempotent: false },
            Contract { name: "status".into(), input_schema: "{\"release_id\":str}".into(), output_schema: "Release".into(), idempotent: true },
        ],
        authority: AuthorityTier::Admin,
        transports: vec![Transport::InProcess],
        receipts: ReceiptPolicy::Always,
        entrypoint: "psw_cockpit::Cockpit".into(),
    }
}

impl Organelle for Cockpit {
    fn name(&self) -> &'static str {
        "psw-cockpit"
    }
    fn manifest(&self) -> OrganelleManifest {
        manifest()
    }
    fn call(&self, contract: &str, input: Value) -> Result<Value, OrganelleError> {
        let rid = || input.get("release_id").and_then(|v| v.as_str()).ok_or_else(|| OrganelleError::BadInput("release_id".into()));
        match contract {
            "plan" => {
                let name = input.get("name").and_then(|v| v.as_str()).ok_or_else(|| OrganelleError::BadInput("name".into()))?;
                let stages: Vec<String> = serde_json::from_value(input.get("stages").cloned().unwrap_or(Value::Null))
                    .map_err(|e| OrganelleError::BadInput(e.to_string()))?;
                let id = self.plan_release(name, stages).map_err(|e| OrganelleError::Execution(e.to_string()))?;
                Ok(serde_json::json!({"release_id": id}))
            }
            "start" => {
                let stage = input.get("stage").and_then(|v| v.as_str()).ok_or_else(|| OrganelleError::BadInput("stage".into()))?;
                self.start_stage(rid()?, stage).map_err(|e| OrganelleError::Execution(e.to_string()))?;
                Ok(serde_json::json!({"ok": true}))
            }
            "finish" => {
                let stage = input.get("stage").and_then(|v| v.as_str()).ok_or_else(|| OrganelleError::BadInput("stage".into()))?;
                let ok = input.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
                let note = input.get("note").and_then(|v| v.as_str()).unwrap_or("");
                self.finish_stage(rid()?, stage, ok, note).map_err(|e| OrganelleError::Execution(e.to_string()))?;
                Ok(serde_json::json!({"ok": true}))
            }
            "status" => {
                let r = self.status(rid()?).map_err(|e| OrganelleError::Execution(e.to_string()))?;
                Ok(serde_json::to_value(r).unwrap())
            }
            _ => Err(OrganelleError::UnknownContract(contract.into(), self.name().into())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_release_lifecycle() {
        let c = Cockpit::new();
        let id = c.plan_release("r1", vec!["build".into(), "test".into()]).unwrap();
        c.start_stage(&id, "build").unwrap();
        c.finish_stage(&id, "build", true, "ok").unwrap();
        c.start_stage(&id, "test").unwrap();
        c.finish_stage(&id, "test", true, "ok").unwrap();
        let r = c.status(&id).unwrap();
        assert!(r.stages.iter().all(|s| s.state == StageState::Done));
        assert!(!r.halted);
    }

    #[test]
    fn failure_halts_release() {
        let c = Cockpit::new();
        let id = c.plan_release("r2", vec!["a".into(), "b".into()]).unwrap();
        c.start_stage(&id, "a").unwrap();
        c.finish_stage(&id, "a", false, "boom").unwrap();
        let r = c.status(&id).unwrap();
        assert!(r.halted);
        assert!(matches!(c.start_stage(&id, "b"), Err(CockpitError::Halted(_, _))));
    }

    #[test]
    fn cannot_finish_unstarted() {
        let c = Cockpit::new();
        let id = c.plan_release("r3", vec!["a".into()]).unwrap();
        assert!(c.finish_stage(&id, "a", true, "").is_err());
    }

    #[test]
    fn cannot_restart_terminal() {
        let c = Cockpit::new();
        let id = c.plan_release("r4", vec!["a".into()]).unwrap();
        c.start_stage(&id, "a").unwrap();
        c.finish_stage(&id, "a", true, "").unwrap();
        assert!(c.start_stage(&id, "a").is_err());
    }

    #[test]
    fn unknown_release_errors() {
        let c = Cockpit::new();
        assert!(matches!(c.status("ghost"), Err(CockpitError::UnknownRelease(_))));
    }

    #[test]
    fn no_stages_refused() {
        let c = Cockpit::new();
        assert!(matches!(c.plan_release("r", vec![]), Err(CockpitError::NoStages)));
    }

    #[test]
    fn organelle_plan_status() {
        let c = Cockpit::new();
        let out = c.call("plan", serde_json::json!({"name": "r", "stages": ["s1"]})).unwrap();
        let rid = out["release_id"].as_str().unwrap();
        let st = c.call("status", serde_json::json!({"release_id": rid})).unwrap();
        assert_eq!(st["stages"][0]["state"], "PRE");
        assert_eq!(st["halted"], false);
    }
}
