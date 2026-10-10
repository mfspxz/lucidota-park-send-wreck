//! psw-quarantine as an organelle (Tier A native migration). Untrusted payload containment.
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
use crate::Quarantine;

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


fn psw_quarantine_manifest() -> OrganelleManifest {
    OrganelleManifest {
        schema_version: MANIFEST_SCHEMA_VERSION.to_string(),
        name: "psw-quarantine".to_string(),
        codon: "qn".to_string(),
        version: "0.1.0".to_string(),
        description: "containment for untrusted payloads: inspect scores risk, contain decides (organelle)".to_string(),
        contracts: vec![
            Contract {
                op: "inspect".to_string(),
                description: "score a payload for risk (pure; never executes it)".to_string(),
                input_schema: serde_json::json!({"type":"object","required":["payload"],"properties":{"payload":{}}}),
                mutation_class: None,
                required_capability: None,
            },
            Contract {
                op: "contain".to_string(),
                description: "containment decision for a payload (id on accept; report on refuse)".to_string(),
                input_schema: serde_json::json!({"type":"object","required":["payload"],"properties":{"payload":{}}}),
                mutation_class: Some(MutationClass::Write),
                required_capability: Some("psw.quarantine.write".to_string()),
            },
        ],
        authority: Authority {
            mutation_class: MutationClass::Write,
            declared_valence: Valence::Candidate,
            required_capability: Some("psw.quarantine.write".to_string()),
            granted_capabilities: vec!["psw.quarantine.write".to_string()],
        },
        transports: vec![Transport::Native, Transport::UnixSocket],
        receipt_policy: ReceiptPolicy {
            content_addressed: true,
            algorithm: RECEIPT_ALGORITHM.to_string(),
            chain: true,
        },
    }
}

pub struct PswQuarantineOrg {
    manifest: OrganelleManifest,
    inner: Quarantine,
}

pub fn psw_quarantine_organelle() -> Arc<dyn Organelle> {
    Arc::new(PswQuarantineOrg {
        manifest: psw_quarantine_manifest(),
            inner: Quarantine,
    })
}

impl Organelle for PswQuarantineOrg {
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
            "inspect" => {
                let r = self.inner.inspect(&input["payload"]);
                                serde_json::to_value(&r)
                                    .map_err(|e| OrganelleError::failed(format!("report not JSON: {e}")))
            },
            "contain" => {
                match self.inner.contain(&input["payload"]) {
                                    Ok(id) => Ok(serde_json::json!({"contained": true, "id": id})),
                                    Err(report) => {
                                        let r = serde_json::to_value(&report)
                                            .map_err(|e| OrganelleError::failed(format!("report not JSON: {e}")))?;
                                        Ok(serde_json::json!({"contained": false, "report": r}))
                                    }
                                }
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
    fn psw_quarantine_static_manifest_validates() {
        let v: Value = serde_json::from_str(include_str!("../../../organelles/psw-quarantine.manifest.json")).expect("manifest JSON parses");
        let m = validate_manifest_json(&v).expect("static manifest must validate");
        assert_eq!(m.name, "psw-quarantine");
        assert_eq!(m.codon, "qn");
    }

    #[test]
    fn psw_quarantine_rust_manifest_matches_static_json() {
        let from_rust = serde_json::to_value(psw_quarantine_manifest()).unwrap();
        let from_file: Value = serde_json::from_str(include_str!("../../../organelles/psw-quarantine.manifest.json")).unwrap();
        assert_eq!(from_rust, from_file, "Rust manifest() and organelles/*.json must agree");
    }

    #[test]
    fn psw_quarantine_registers_and_serves_through_host_with_receipts() {
        let host = Host::new();
        host.register(psw_quarantine_organelle()).expect("registration must succeed");

        let ctx = CallContext::new("migration-test", vec!["psw.quarantine.write".to_string()]);
        let out = host
            .call(&ctx, "qn", "inspect", serde_json::json!({"payload": {"kind": "text", "body": "hello"}}))
            .expect("inspect call");
        assert!(out.get("risk_score").is_some());
        let out = host
            .call(&ctx, "qn", "contain", serde_json::json!({"payload": {"kind": "text", "body": "hello"}}))
            .expect("contain call");
        assert!(out.get("contained").and_then(|v| v.as_bool()).is_some());
        let n = host.verify_chain().expect("receipt chain must verify");
        assert!(n >= 2, "register + calls = >=2 receipts, got {n}");
    }
}
