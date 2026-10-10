//! psw-cockpit as an organelle (Tier A native migration). Release stage management.
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
use crate::Cockpit;

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


fn psw_cockpit_manifest() -> OrganelleManifest {
    OrganelleManifest {
        schema_version: MANIFEST_SCHEMA_VERSION.to_string(),
        name: "psw-cockpit".to_string(),
        codon: "ck".to_string(),
        version: "0.1.0".to_string(),
        description: "operator cockpit: named release stage sequences with status (organelle)".to_string(),
        contracts: vec![
            Contract {
                op: "plan_release".to_string(),
                description: "plan a named release as a stage sequence; returns the release id".to_string(),
                input_schema: serde_json::json!({"type":"object","required":["name","stages"],"properties":{"name":{"type":"string"},"stages":{"type":"array","items":{"type":"string"}}}}),
                mutation_class: Some(MutationClass::Write),
                required_capability: Some("psw.cockpit.write".to_string()),
            },
            Contract {
                op: "start_stage".to_string(),
                description: "start one stage of a release".to_string(),
                input_schema: serde_json::json!({"type":"object","required":["id","stage"],"properties":{"id":{"type":"string"},"stage":{"type":"string"}}}),
                mutation_class: Some(MutationClass::Write),
                required_capability: Some("psw.cockpit.write".to_string()),
            },
            Contract {
                op: "finish_stage".to_string(),
                description: "finish one stage with ok flag + note".to_string(),
                input_schema: serde_json::json!({"type":"object","required":["id","stage","ok"],"properties":{"id":{"type":"string"},"stage":{"type":"string"},"ok":{"type":"boolean"},"note":{"type":"string"}}}),
                mutation_class: Some(MutationClass::Write),
                required_capability: Some("psw.cockpit.write".to_string()),
            },
            Contract {
                op: "status".to_string(),
                description: "release status".to_string(),
                input_schema: serde_json::json!({"type":"object","required":["id"],"properties":{"id":{"type":"string"}}}),
                mutation_class: None,
                required_capability: None,
            },
        ],
        authority: Authority {
            mutation_class: MutationClass::Write,
            declared_valence: Valence::Candidate,
            required_capability: Some("psw.cockpit.write".to_string()),
            granted_capabilities: vec!["psw.cockpit.write".to_string()],
        },
        transports: vec![Transport::Native, Transport::UnixSocket],
        receipt_policy: ReceiptPolicy {
            content_addressed: true,
            algorithm: RECEIPT_ALGORITHM.to_string(),
            chain: true,
        },
    }
}

pub struct PswCockpitOrg {
    manifest: OrganelleManifest,
    inner: Mutex<Cockpit>,
}

pub fn psw_cockpit_organelle() -> Arc<dyn Organelle> {
    Arc::new(PswCockpitOrg {
        manifest: psw_cockpit_manifest(),
            inner: Mutex::new(Cockpit::new()),
    })
}

impl Organelle for PswCockpitOrg {
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
            "plan_release" => {
                let stages: Vec<String> = serde_json::from_value(input["stages"].clone())
                                    .map_err(|e| OrganelleError::failed(format!("bad stages: {e}")))?;
                                let inner = self.inner.lock().unwrap();
                                let id = inner.plan_release(req(&input, "name")?, stages)
                                    .map_err(|e| OrganelleError::failed(format!("plan_release failed: {e}")))?;
                                Ok(serde_json::json!({"id": id}))
            },
            "start_stage" => {
                let inner = self.inner.lock().unwrap();
                                inner.start_stage(req(&input, "id")?, req(&input, "stage")?)
                                    .map_err(|e| OrganelleError::failed(format!("start_stage failed: {e}")))?;
                                Ok(serde_json::json!({"ok": true}))
            },
            "finish_stage" => {
                let inner = self.inner.lock().unwrap();
                                inner.finish_stage(req(&input, "id")?, req(&input, "stage")?,
                                        input.get("ok").and_then(|v| v.as_bool()).unwrap_or(false),
                                        opt_str(&input, "note").unwrap_or_default().as_str())
                                    .map_err(|e| OrganelleError::failed(format!("finish_stage failed: {e}")))?;
                                Ok(serde_json::json!({"ok": true}))
            },
            "status" => {
                let inner = self.inner.lock().unwrap();
                                let r = inner.status(req(&input, "id")?)
                                    .map_err(|e| OrganelleError::not_found(format!("status failed: {e}")))?;
                                serde_json::to_value(&r)
                                    .map_err(|e| OrganelleError::failed(format!("not JSON: {e}")))
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
    fn psw_cockpit_static_manifest_validates() {
        let v: Value = serde_json::from_str(include_str!("../../../organelles/psw-cockpit.manifest.json")).expect("manifest JSON parses");
        let m = validate_manifest_json(&v).expect("static manifest must validate");
        assert_eq!(m.name, "psw-cockpit");
        assert_eq!(m.codon, "ck");
    }

    #[test]
    fn psw_cockpit_rust_manifest_matches_static_json() {
        let from_rust = serde_json::to_value(psw_cockpit_manifest()).unwrap();
        let from_file: Value = serde_json::from_str(include_str!("../../../organelles/psw-cockpit.manifest.json")).unwrap();
        assert_eq!(from_rust, from_file, "Rust manifest() and organelles/*.json must agree");
    }

    #[test]
    fn psw_cockpit_registers_and_serves_through_host_with_receipts() {
        let host = Host::new();
        host.register(psw_cockpit_organelle()).expect("registration must succeed");

        let ctx = CallContext::new("migration-test", vec!["psw.cockpit.write".to_string()]);
        let out = host.call(&ctx, "ck", "plan_release",
            serde_json::json!({"name": "r1", "stages": ["build", "ship"]})).expect("plan");
        let id = out.get("id").and_then(|v| v.as_str()).unwrap().to_string();
        host.call(&ctx, "ck", "start_stage",
            serde_json::json!({"id": id, "stage": "build"})).expect("start");
        let st = host.call(&ctx, "ck", "status",
            serde_json::json!({"id": id})).expect("status");
        assert_eq!(st.get("name").and_then(|v| v.as_str()), Some("r1"));
        let n = host.verify_chain().expect("receipt chain must verify");
        assert!(n >= 2, "register + calls = >=2 receipts, got {n}");
    }
}
