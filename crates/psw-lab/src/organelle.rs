//! psw-lab as an organelle (Tier A native migration). Pre-registered experiments.
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
use crate::{Lab, PreReg};

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


fn psw_lab_manifest() -> OrganelleManifest {
    OrganelleManifest {
        schema_version: MANIFEST_SCHEMA_VERSION.to_string(),
        name: "psw-lab".to_string(),
        codon: "lb".to_string(),
        version: "0.1.0".to_string(),
        description: "experiment sandbox: no run without a committed pre-registration (organelle)".to_string(),
        contracts: vec![
            Contract {
                op: "preregister".to_string(),
                description: "commit a pre-registration (hypothesis + method + falsifier)".to_string(),
                input_schema: serde_json::json!({"type":"object","required":["exp_id","hypothesis","method","falsifier"],"properties":{"exp_id":{"type":"string"},"hypothesis":{"type":"string"},"method":{"type":"string"},"falsifier":{"type":"string"}}}),
                mutation_class: Some(MutationClass::Write),
                required_capability: Some("psw.lab.write".to_string()),
            },
            Contract {
                op: "run".to_string(),
                description: "record a run against its pre-registration (pre-reg never mutates)".to_string(),
                input_schema: serde_json::json!({"type":"object","required":["exp_id","result"],"properties":{"exp_id":{"type":"string"},"result":{"type":"string"},"notes":{"type":"string"}}}),
                mutation_class: Some(MutationClass::Write),
                required_capability: Some("psw.lab.write".to_string()),
            },
            Contract {
                op: "runs_for".to_string(),
                description: "run records for one experiment".to_string(),
                input_schema: serde_json::json!({"type":"object","required":["exp_id"],"properties":{"exp_id":{"type":"string"}}}),
                mutation_class: None,
                required_capability: None,
            },
        ],
        authority: Authority {
            mutation_class: MutationClass::Write,
            declared_valence: Valence::Candidate,
            required_capability: Some("psw.lab.write".to_string()),
            granted_capabilities: vec!["psw.lab.write".to_string()],
        },
        transports: vec![Transport::Native, Transport::UnixSocket],
        receipt_policy: ReceiptPolicy {
            content_addressed: true,
            algorithm: RECEIPT_ALGORITHM.to_string(),
            chain: true,
        },
    }
}

pub struct PswLabOrg {
    manifest: OrganelleManifest,
    inner: Mutex<Lab>,
}

pub fn psw_lab_organelle() -> Arc<dyn Organelle> {
    Arc::new(PswLabOrg {
        manifest: psw_lab_manifest(),
            inner: Mutex::new(Lab::new()),
    })
}

impl Organelle for PswLabOrg {
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
            "preregister" => {
                let p = PreReg {
                                    exp_id: req(&input, "exp_id")?.to_string(),
                                    hypothesis: req(&input, "hypothesis")?.to_string(),
                                    method: req(&input, "method")?.to_string(),
                                    falsifier: req(&input, "falsifier")?.to_string(),
                                };
                                let inner = self.inner.lock().unwrap();
                                inner.preregister(p)
                                    .map_err(|e| OrganelleError::failed(format!("preregister failed: {e}")))?;
                                Ok(serde_json::json!({"ok": true}))
            },
            "run" => {
                let inner = self.inner.lock().unwrap();
                                let r = inner.run(req(&input, "exp_id")?, req(&input, "result")?,
                                        opt_str(&input, "notes").unwrap_or_default().as_str())
                                    .map_err(|e| OrganelleError::failed(format!("run failed: {e}")))?;
                                serde_json::to_value(&r)
                                    .map_err(|e| OrganelleError::failed(format!("run record not JSON: {e}")))
            },
            "runs_for" => {
                let inner = self.inner.lock().unwrap();
                                let v = inner.runs_for(req(&input, "exp_id")?);
                                serde_json::to_value(&v)
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
    fn psw_lab_static_manifest_validates() {
        let v: Value = serde_json::from_str(include_str!("../../../organelles/psw-lab.manifest.json")).expect("manifest JSON parses");
        let m = validate_manifest_json(&v).expect("static manifest must validate");
        assert_eq!(m.name, "psw-lab");
        assert_eq!(m.codon, "lb");
    }

    #[test]
    fn psw_lab_rust_manifest_matches_static_json() {
        let from_rust = serde_json::to_value(psw_lab_manifest()).unwrap();
        let from_file: Value = serde_json::from_str(include_str!("../../../organelles/psw-lab.manifest.json")).unwrap();
        assert_eq!(from_rust, from_file, "Rust manifest() and organelles/*.json must agree");
    }

    #[test]
    fn psw_lab_registers_and_serves_through_host_with_receipts() {
        let host = Host::new();
        host.register(psw_lab_organelle()).expect("registration must succeed");

        let ctx = CallContext::new("migration-test", vec!["psw.lab.write".to_string()]);
        let out = host
            .call(&ctx, "lb", "preregister", serde_json::json!({"exp_id": "e1", "hypothesis": "h", "method": "m", "falsifier": "f"}))
            .expect("preregister call");
        assert_eq!(out.get("ok").and_then(|v| v.as_bool()), Some(true));
        let out = host
            .call(&ctx, "lb", "run", serde_json::json!({"exp_id": "e1", "result": "held"}))
            .expect("run call");
        assert!(out.is_object());
        let out = host
            .call(&ctx, "lb", "runs_for", serde_json::json!({"exp_id": "e1"}))
            .expect("runs_for call");
        assert_eq!(out.as_array().map(|a| a.len()), Some(1));
        assert!(host.call(&ctx, "lb", "run", serde_json::json!({"exp_id": "ghost", "result": "x"})).is_err());
        let n = host.verify_chain().expect("receipt chain must verify");
        assert!(n >= 2, "register + calls = >=2 receipts, got {n}");
    }
}
