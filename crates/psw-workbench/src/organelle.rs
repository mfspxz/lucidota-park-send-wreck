//! psw-workbench as an organelle (Tier A native migration). Validated flow composition.
//!
//! Tier A native migration: implements `organelle_host::Organelle` over the
//! existing crate API. The computation is unchanged; this file only adds the
//! capability contract, manifest, and host registration.

use organelle_host::{CallContext, HostRef, Organelle, OrganelleError};
use organelle_manifest::{
    Authority, Contract, MutationClass, OrganelleManifest, ReceiptPolicy, Transport, Valence,
    MANIFEST_SCHEMA_VERSION, RECEIPT_ALGORITHM,
};
use serde_json::Value;
use std::sync::{Arc, Mutex};
use crate::{Flow, Workbench};
use std::collections::HashMap;

fn req<'a>(input: &'a Value, key: &str) -> Result<&'a str, OrganelleError> {
    input
        .get(key)
        .and_then(|v| v.as_str())
        .ok_or_else(|| OrganelleError::failed(format!("missing required input: {key}")))
}

fn opt_str(input: &Value, key: &str) -> Option<String> {
    input.get(key).and_then(|v| v.as_str()).map(|s| s.to_string())
}

fn opt_f64(input: &Value, key: &str) -> Option<f64> {
    input.get(key).and_then(|v| v.as_f64())
}


fn psw_workbench_manifest() -> OrganelleManifest {
    OrganelleManifest {
        schema_version: MANIFEST_SCHEMA_VERSION.to_string(),
        name: "psw-workbench".to_string(),
        codon: "wb".to_string(),
        version: "0.1.0".to_string(),
        description: "composition canvas: validate a flow DAG before anything executes (organelle)".to_string(),
        contracts: vec![
            Contract {
                op: "compose".to_string(),
                description: "validate a flow against the organelle catalog; refuse unknown organelles, undeclared contracts, cycles".to_string(),
                input_schema: serde_json::json!({"type":"object","required":["flow","catalog"],"properties":{"flow":{"type":"object"},"catalog":{"type":"object"}}}),
                mutation_class: None,
                required_capability: None,
            },
        ],
        authority: Authority {
            mutation_class: MutationClass::ReadOnly,
            declared_valence: Valence::Proven,
            required_capability: None,
            granted_capabilities: vec![],
        },
        transports: vec![Transport::Native, Transport::UnixSocket, Transport::Mcp, Transport::Rest],
        receipt_policy: ReceiptPolicy {
            content_addressed: true,
            algorithm: RECEIPT_ALGORITHM.to_string(),
            chain: true,
        },
    }
}

pub struct PswWorkbenchOrg {
    manifest: OrganelleManifest,
    _priv: (),
}

pub fn psw_workbench_organelle() -> Arc<dyn Organelle> {
    Arc::new(PswWorkbenchOrg {
        manifest: psw_workbench_manifest(),
            _priv: (),
    })
}

impl Organelle for PswWorkbenchOrg {
    fn manifest(&self) -> OrganelleManifest {
        self.manifest.clone()
    }

    fn handle(
        &self,
        _ctx: &CallContext,
        _host: &dyn HostRef,
        op: &str,
        input: Value,
    ) -> Result<Value, OrganelleError> {
        match op {
            "compose" => {
                let flow: Flow = serde_json::from_value(input["flow"].clone())
                                    .map_err(|e| OrganelleError::failed(format!("bad flow: {e}")))?;
                                let catalog: HashMap<String, Vec<String>> = serde_json::from_value(input["catalog"].clone())
                                    .map_err(|e| OrganelleError::failed(format!("bad catalog: {e}")))?;
                                let plan = Workbench::compose(&flow, &catalog)
                                    .map_err(|e| OrganelleError::failed(format!("compose refused: {e}")))?;
                                serde_json::to_value(&plan)
                                    .map_err(|e| OrganelleError::failed(format!("plan not JSON: {e}")))
            },
            _ => Err(OrganelleError::not_found(format!("unknown op {op}"))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use organelle_host::Host;
    use organelle_manifest::validate_manifest_json;

    #[test]
    fn psw_workbench_static_manifest_validates() {
        let v: Value = serde_json::from_str(include_str!("../../../organelles/psw-workbench.manifest.json")).expect("manifest JSON parses");
        let m = validate_manifest_json(&v).expect("static manifest must validate");
        assert_eq!(m.name, "psw-workbench");
        assert_eq!(m.codon, "wb");
    }

    #[test]
    fn psw_workbench_rust_manifest_matches_static_json() {
        let from_rust = serde_json::to_value(psw_workbench_manifest()).unwrap();
        let from_file: Value = serde_json::from_str(include_str!("../../../organelles/psw-workbench.manifest.json")).unwrap();
        assert_eq!(from_rust, from_file, "Rust manifest() and organelles/*.json must agree");
    }

    #[test]
    fn psw_workbench_registers_and_serves_through_host_with_receipts() {
        let host = Host::new();
        host.register(psw_workbench_organelle()).expect("registration must succeed");

        let ctx = CallContext::new("migration-test", vec![]);
        let out = host
            .call(&ctx, "wb", "compose", serde_json::json!({"flow": {"name": "f", "steps": [{"id": "s1", "organelle": "psw-forge", "contract": "build", "input": {}}]}, "catalog": {"psw-forge": ["build"]}}))
            .expect("compose call");
        assert!(out.get("name").is_some());
        // unknown organelle must be refused, not executed
        assert!(host.call(&ctx, "wb", "compose", serde_json::json!(
            {"flow": {"name": "f", "steps": [{"id": "s1", "organelle": "nope", "contract": "x", "input": {}}]},
             "catalog": {"psw-forge": ["build"]}})).is_err());
        let n = host.verify_chain().expect("receipt chain must verify");
        assert!(n >= 2, "register + calls = >=2 receipts, got {n}");
    }
}
