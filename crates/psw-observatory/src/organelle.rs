//! psw-observatory as an organelle (Tier A native migration). Append-only telemetry stream.
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
use crate::Observatory;

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


fn psw_observatory_manifest() -> OrganelleManifest {
    OrganelleManifest {
        schema_version: MANIFEST_SCHEMA_VERSION.to_string(),
        name: "psw-observatory".to_string(),
        codon: "ob".to_string(),
        version: "0.1.0".to_string(),
        description: "telemetry for the PSW: append-only event stream with kind-filtered queries (organelle)".to_string(),
        contracts: vec![
            Contract {
                op: "emit".to_string(),
                description: "append one telemetry event".to_string(),
                input_schema: serde_json::json!({"type":"object","required":["kind","source"],"properties":{"kind":{"type":"string"},"source":{"type":"string"},"payload":{"type":"object"}}}),
                mutation_class: Some(MutationClass::Write),
                required_capability: Some("psw.observatory.write".to_string()),
            },
            Contract {
                op: "query".to_string(),
                description: "events by kind, from a seq, with limit".to_string(),
                input_schema: serde_json::json!({"type":"object","properties":{"kind":{"type":"string"},"since_seq":{"type":"integer"},"limit":{"type":"integer"}}}),
                mutation_class: None,
                required_capability: None,
            },
            Contract {
                op: "tail".to_string(),
                description: "last n events".to_string(),
                input_schema: serde_json::json!({"type":"object","properties":{"n":{"type":"integer"}}}),
                mutation_class: None,
                required_capability: None,
            },
            Contract {
                op: "count".to_string(),
                description: "event count".to_string(),
                input_schema: serde_json::json!({"type":"object"}),
                mutation_class: None,
                required_capability: None,
            },
        ],
        authority: Authority {
            mutation_class: MutationClass::Write,
            declared_valence: Valence::Candidate,
            required_capability: Some("psw.observatory.write".to_string()),
            granted_capabilities: vec!["psw.observatory.write".to_string()],
        },
        transports: vec![Transport::Native, Transport::UnixSocket],
        receipt_policy: ReceiptPolicy {
            content_addressed: true,
            algorithm: RECEIPT_ALGORITHM.to_string(),
            chain: true,
        },
    }
}

pub struct PswObservatoryOrg {
    manifest: OrganelleManifest,
    inner: Observatory,
}

pub fn psw_observatory_organelle() -> Arc<dyn Organelle> {
    Arc::new(PswObservatoryOrg {
        manifest: psw_observatory_manifest(),
            inner: Observatory::new(),
    })
}

impl Organelle for PswObservatoryOrg {
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
            "emit" => {
                let e = self.inner.emit(req(&input, "kind")?, req(&input, "source")?,
                                    input.get("payload").cloned().unwrap_or(serde_json::json!({})));
                                serde_json::to_value(&e)
                                    .map_err(|e| OrganelleError::failed(format!("event not JSON: {e}")))
            },
            "query" => {
                let v = self.inner.query(opt_str(&input, "kind").as_deref(),
                                    input.get("since_seq").and_then(|x| x.as_u64()).unwrap_or(0),
                                    input.get("limit").and_then(|x| x.as_u64()).unwrap_or(100) as usize);
                                serde_json::to_value(&v)
                                    .map_err(|e| OrganelleError::failed(format!("not JSON: {e}")))
            },
            "tail" => {
                let v = self.inner.tail(input.get("n").and_then(|x| x.as_u64()).unwrap_or(10) as usize);
                                serde_json::to_value(&v)
                                    .map_err(|e| OrganelleError::failed(format!("not JSON: {e}")))
            },
            "count" => {
                Ok(serde_json::json!({"count": self.inner.count()}))
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
    fn psw_observatory_static_manifest_validates() {
        let v: Value = serde_json::from_str(include_str!("../../../organelles/psw-observatory.manifest.json")).expect("manifest JSON parses");
        let m = validate_manifest_json(&v).expect("static manifest must validate");
        assert_eq!(m.name, "psw-observatory");
        assert_eq!(m.codon, "ob");
    }

    #[test]
    fn psw_observatory_rust_manifest_matches_static_json() {
        let from_rust = serde_json::to_value(psw_observatory_manifest()).unwrap();
        let from_file: Value = serde_json::from_str(include_str!("../../../organelles/psw-observatory.manifest.json")).unwrap();
        assert_eq!(from_rust, from_file, "Rust manifest() and organelles/*.json must agree");
    }

    #[test]
    fn psw_observatory_registers_and_serves_through_host_with_receipts() {
        let host = Host::new();
        host.register(psw_observatory_organelle()).expect("registration must succeed");

        let ctx = CallContext::new("migration-test", vec!["psw.observatory.write".to_string()]);
        let out = host
            .call(&ctx, "ob", "emit", serde_json::json!({"kind": "build", "source": "forge", "payload": {"ok": true}}))
            .expect("emit call");
        assert_eq!(out.get("seq").and_then(|v| v.as_u64()), Some(1));
        let out = host
            .call(&ctx, "ob", "query", serde_json::json!({"kind": "build"}))
            .expect("query call");
        assert_eq!(out.as_array().map(|a| a.len()), Some(1));
        let out = host
            .call(&ctx, "ob", "count", serde_json::json!({}))
            .expect("count call");
        assert_eq!(out.get("count").and_then(|v| v.as_u64()), Some(1));
        let n = host.verify_chain().expect("receipt chain must verify");
        assert!(n >= 2, "register + calls = >=2 receipts, got {n}");
    }
}
