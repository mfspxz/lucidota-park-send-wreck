//! psw-001 — Workbench: the composition canvas.
//! Organelles are wired into flows (DAGs of calls). The workbench validates
//! a flow BEFORE anything executes: unknown organelles, undeclared contracts,
//! and cycles are all refused. This is the feat/workbench-ontological-execution
//! branch made concrete: execution follows validated composition.
use pw_domain::AuthorityTier;
use pw_manifest::{Contract, OrganelleManifest, ReceiptPolicy, Transport, SCHEMA_VERSION};
use pw_organelles::{Organelle, OrganelleError};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowStep {
    pub id: String,
    pub organelle: String,
    pub contract: String,
    pub input: Value,
    #[serde(default)]
    pub after: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Flow {
    pub name: String,
    pub steps: Vec<FlowStep>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowPlan {
    pub name: String,
    /// Step ids in execution order (topological).
    pub order: Vec<String>,
    pub step_count: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum WorkbenchError {
    #[error("unknown organelle {0:?} in step {1:?}")]
    UnknownOrganelle(String, String),
    #[error("undeclared contract {0:?} on {1:?} in step {2:?}")]
    UndeclaredContract(String, String, String),
    #[error("cycle detected involving step {0:?}")]
    Cycle(String),
    #[error("unknown dependency {0:?} in step {1:?}")]
    UnknownDependency(String, String),
    #[error("duplicate step id {0:?}")]
    DuplicateStep(String),
    #[error("empty flow")]
    EmptyFlow,
}

pub struct Workbench;

impl Workbench {
    /// Validate + topologically sort. `catalog`: organelle -> declared contracts.
    pub fn compose(flow: &Flow, catalog: &HashMap<String, Vec<String>>) -> Result<FlowPlan, WorkbenchError> {
        if flow.steps.is_empty() {
            return Err(WorkbenchError::EmptyFlow);
        }
        let mut ids = HashSet::new();
        for s in &flow.steps {
            if !ids.insert(s.id.clone()) {
                return Err(WorkbenchError::DuplicateStep(s.id.clone()));
            }
            let contracts = catalog.get(&s.organelle)
                .ok_or_else(|| WorkbenchError::UnknownOrganelle(s.organelle.clone(), s.id.clone()))?;
            if !contracts.contains(&s.contract) {
                return Err(WorkbenchError::UndeclaredContract(s.contract.clone(), s.organelle.clone(), s.id.clone()));
            }
            for dep in &s.after {
                if !flow.steps.iter().any(|x| &x.id == dep) {
                    return Err(WorkbenchError::UnknownDependency(dep.clone(), s.id.clone()));
                }
            }
        }
        // Kahn's algorithm.
        let mut indeg: HashMap<&str, usize> = HashMap::new();
        let mut dependents: HashMap<&str, Vec<&str>> = HashMap::new();
        for s in &flow.steps {
            indeg.insert(s.id.as_str(), s.after.len());
            for dep in &s.after {
                dependents.entry(dep.as_str()).or_default().push(s.id.as_str());
            }
        }
        let mut ready: Vec<&str> = indeg.iter().filter(|(_, &d)| d == 0).map(|(&k, _)| k).collect();
        ready.sort_unstable();
        let mut order = Vec::new();
        while let Some(n) = ready.pop() {
            order.push(n.to_string());
            if let Some(ds) = dependents.get(n) {
                let mut ds = ds.clone();
                ds.sort_unstable();
                for d in ds {
                    let e = indeg.get_mut(d).unwrap();
                    *e -= 1;
                    if *e == 0 {
                        ready.push(d);
                    }
                }
            }
            ready.sort_unstable();
        }
        if order.len() != flow.steps.len() {
            let stuck = flow.steps.iter().find(|s| !order.contains(&s.id)).unwrap();
            return Err(WorkbenchError::Cycle(stuck.id.clone()));
        }
        Ok(FlowPlan { name: flow.name.clone(), order, step_count: flow.steps.len() })
    }
}

fn manifest() -> OrganelleManifest {
    OrganelleManifest {
        schema: SCHEMA_VERSION.into(),
        name: "psw-workbench".into(),
        version: "0.1.0".into(),
        description: "Composition canvas: validate organelle flows before execution".into(),
        contracts: vec![
            Contract { name: "compose".into(), input_schema: "Flow+catalog".into(), output_schema: "FlowPlan".into(), idempotent: true },
            Contract { name: "describe".into(), input_schema: "{}".into(), output_schema: "manifest".into(), idempotent: true },
        ],
        authority: AuthorityTier::Operator,
        transports: vec![Transport::InProcess],
        receipts: ReceiptPolicy::Always,
        entrypoint: "psw_workbench::Workbench".into(),
    }
}

impl Organelle for Workbench {
    fn name(&self) -> &'static str {
        "psw-workbench"
    }
    fn manifest(&self) -> OrganelleManifest {
        manifest()
    }
    fn call(&self, contract: &str, input: Value) -> Result<Value, OrganelleError> {
        match contract {
            "describe" => Ok(serde_json::to_value(manifest()).unwrap()),
            "compose" => {
                let flow: Flow = serde_json::from_value(input.get("flow").cloned().unwrap_or(Value::Null))
                    .map_err(|e| OrganelleError::BadInput(e.to_string()))?;
                let catalog: HashMap<String, Vec<String>> =
                    serde_json::from_value(input.get("catalog").cloned().unwrap_or(Value::Null))
                        .map_err(|e| OrganelleError::BadInput(e.to_string()))?;
                let plan = Workbench::compose(&flow, &catalog)
                    .map_err(|e| OrganelleError::Execution(e.to_string()))?;
                Ok(serde_json::to_value(plan).unwrap())
            }
            _ => Err(OrganelleError::UnknownContract(contract.into(), self.name().into())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog() -> HashMap<String, Vec<String>> {
        [("psw-archive".into(), vec!["put".into(), "get".into()]),
         ("psw-forge".into(), vec!["build".into()])]
            .into_iter()
            .collect()
    }

    fn flow(steps: Vec<FlowStep>) -> Flow {
        Flow { name: "f".into(), steps }
    }
    fn step(id: &str, org: &str, contract: &str, after: &[&str]) -> FlowStep {
        FlowStep { id: id.into(), organelle: org.into(), contract: contract.into(),
                   input: Value::Null, after: after.iter().map(|s| s.to_string()).collect() }
    }

    #[test]
    fn linear_flow_orders() {
        let f = flow(vec![step("a", "psw-archive", "put", &[]), step("b", "psw-forge", "build", &["a"])]);
        let plan = Workbench::compose(&f, &catalog()).unwrap();
        assert_eq!(plan.order, vec!["a", "b"]);
    }

    #[test]
    fn diamond_orders_deps_first() {
        let f = flow(vec![
            step("a", "psw-archive", "put", &[]),
            step("b", "psw-forge", "build", &["a"]),
            step("c", "psw-archive", "get", &["a"]),
            step("d", "psw-forge", "build", &["b", "c"]),
        ]);
        let plan = Workbench::compose(&f, &catalog()).unwrap();
        let pos = |id: &str| plan.order.iter().position(|x| x == id).unwrap();
        assert!(pos("a") < pos("b") && pos("a") < pos("c") && pos("b") < pos("d") && pos("c") < pos("d"));
    }

    #[test]
    fn unknown_organelle_refused() {
        let f = flow(vec![step("a", "psw-nope", "put", &[])]);
        assert!(matches!(Workbench::compose(&f, &catalog()), Err(WorkbenchError::UnknownOrganelle(_, _))));
    }

    #[test]
    fn undeclared_contract_refused() {
        let f = flow(vec![step("a", "psw-archive", "nope", &[])]);
        assert!(matches!(Workbench::compose(&f, &catalog()), Err(WorkbenchError::UndeclaredContract(_, _, _))));
    }

    #[test]
    fn cycle_refused() {
        let f = flow(vec![step("a", "psw-archive", "put", &["b"]), step("b", "psw-forge", "build", &["a"])]);
        assert!(matches!(Workbench::compose(&f, &catalog()), Err(WorkbenchError::Cycle(_))));
    }

    #[test]
    fn unknown_dependency_refused() {
        let f = flow(vec![step("a", "psw-archive", "put", &["ghost"])]);
        assert!(matches!(Workbench::compose(&f, &catalog()), Err(WorkbenchError::UnknownDependency(_, _))));
    }

    #[test]
    fn duplicate_step_refused() {
        let f = flow(vec![step("a", "psw-archive", "put", &[]), step("a", "psw-forge", "build", &[])]);
        assert!(matches!(Workbench::compose(&f, &catalog()), Err(WorkbenchError::DuplicateStep(_))));
    }

    #[test]
    fn empty_flow_refused() {
        assert!(matches!(Workbench::compose(&flow(vec![]), &catalog()), Err(WorkbenchError::EmptyFlow)));
    }

    #[test]
    fn organelle_call_compose_contract() {
        let w = Workbench;
        let input = serde_json::json!({
            "flow": {"name": "f", "steps": [{"id": "a", "organelle": "psw-archive", "contract": "put", "input": null, "after": []}]},
            "catalog": {"psw-archive": ["put", "get"]}
        });
        let out = w.call("compose", input).unwrap();
        assert_eq!(out["order"], serde_json::json!(["a"]));
    }

    #[test]
    fn organelle_call_unknown_contract() {
        let w = Workbench;
        assert!(w.call("nope", Value::Null).is_err());
    }
}
