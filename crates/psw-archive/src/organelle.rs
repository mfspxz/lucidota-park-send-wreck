//! psw-archive as an organelle (Tier A native migration). Content-addressed vault.
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
use crate::Archive;

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


fn psw_archive_manifest() -> OrganelleManifest {
    OrganelleManifest {
        schema_version: MANIFEST_SCHEMA_VERSION.to_string(),
        name: "psw-archive".to_string(),
        codon: "ar".to_string(),
        version: "0.1.0".to_string(),
        description: "content-addressed vault: put bytes -> blake2 content id; get id -> bytes (organelle)".to_string(),
        contracts: vec![
            Contract {
                op: "put".to_string(),
                description: "store text bytes; returns the blake2-512 content id".to_string(),
                input_schema: serde_json::json!({"type":"object","required":["text"],"properties":{"text":{"type":"string"}}}),
                mutation_class: Some(MutationClass::Write),
                required_capability: Some("psw.archive.write".to_string()),
            },
            Contract {
                op: "get".to_string(),
                description: "retrieve bytes by content id".to_string(),
                input_schema: serde_json::json!({"type":"object","required":["id"],"properties":{"id":{"type":"string"}}}),
                mutation_class: None,
                required_capability: None,
            },
            Contract {
                op: "contains".to_string(),
                description: "whether a content id is stored".to_string(),
                input_schema: serde_json::json!({"type":"object","required":["id"],"properties":{"id":{"type":"string"}}}),
                mutation_class: None,
                required_capability: None,
            },
            Contract {
                op: "len".to_string(),
                description: "stored blob count".to_string(),
                input_schema: serde_json::json!({"type":"object"}),
                mutation_class: None,
                required_capability: None,
            },
        ],
        authority: Authority {
            mutation_class: MutationClass::Write,
            declared_valence: Valence::Candidate,
            required_capability: Some("psw.archive.write".to_string()),
            granted_capabilities: vec!["psw.archive.write".to_string()],
        },
        transports: vec![Transport::Native, Transport::UnixSocket],
        receipt_policy: ReceiptPolicy {
            content_addressed: true,
            algorithm: RECEIPT_ALGORITHM.to_string(),
            chain: true,
        },
    }
}

pub struct PswArchiveOrg {
    manifest: OrganelleManifest,
    inner: Archive,
}

pub fn psw_archive_organelle() -> Arc<dyn Organelle> {
    Arc::new(PswArchiveOrg {
        manifest: psw_archive_manifest(),
            inner: Archive::new(),
    })
}

impl Organelle for PswArchiveOrg {
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
            "put" => {
                let id = self.inner.put(req(&input, "text")?.as_bytes())
                                    .map_err(|e| OrganelleError::failed(format!("put failed: {e}")))?;
                                Ok(serde_json::json!({"id": id}))
            },
            "get" => {
                let bytes = self.inner.get(req(&input, "id")?)
                                    .map_err(|e| OrganelleError::not_found(format!("get failed: {e}")))?;
                                Ok(serde_json::json!({"text": String::from_utf8_lossy(&bytes).to_string()}))
            },
            "contains" => {
                Ok(serde_json::json!({"contains": self.inner.contains(req(&input, "id")?)}))
            },
            "len" => {
                Ok(serde_json::json!({"len": self.inner.len()}))
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
    fn psw_archive_static_manifest_validates() {
        let v: Value = serde_json::from_str(include_str!("../../../organelles/psw-archive.manifest.json")).expect("manifest JSON parses");
        let m = validate_manifest_json(&v).expect("static manifest must validate");
        assert_eq!(m.name, "psw-archive");
        assert_eq!(m.codon, "ar");
    }

    #[test]
    fn psw_archive_rust_manifest_matches_static_json() {
        let from_rust = serde_json::to_value(psw_archive_manifest()).unwrap();
        let from_file: Value = serde_json::from_str(include_str!("../../../organelles/psw-archive.manifest.json")).unwrap();
        assert_eq!(from_rust, from_file, "Rust manifest() and organelles/*.json must agree");
    }

    #[test]
    fn psw_archive_registers_and_serves_through_host_with_receipts() {
        let host = Host::new();
        host.register(psw_archive_organelle()).expect("registration must succeed");

        let ctx = CallContext::new("migration-test", vec!["psw.archive.write".to_string()]);
        let out = host
            .call(&ctx, "ar", "put", serde_json::json!({"text": "hello wreck"}))
            .expect("put call");
        assert!(out.get("id").and_then(|v| v.as_str()).map(|s| s.len()).unwrap_or(0) > 0);
        let id = out.get("id").and_then(|v| v.as_str()).unwrap().to_string();
        let got = host.call(&ctx, "ar", "get", serde_json::json!({"id": id})).expect("get");
        assert_eq!(got.get("text").and_then(|v| v.as_str()), Some("hello wreck"));
        assert!(host.call(&ctx, "ar", "get", serde_json::json!({"id": "nope"})).is_err());
        let n = host.verify_chain().expect("receipt chain must verify");
        assert!(n >= 2, "register + calls = >=2 receipts, got {n}");
    }
}
