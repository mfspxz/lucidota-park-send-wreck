//! psw-forge as an organelle (Tier A native migration). Deterministic artifact builder.
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
use crate::Forge;

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


fn psw_forge_manifest() -> OrganelleManifest {
    OrganelleManifest {
        schema_version: MANIFEST_SCHEMA_VERSION.to_string(),
        name: "psw-forge".to_string(),
        codon: "fg".to_string(),
        version: "0.1.0".to_string(),
        description: "deterministic artifact builder: same spec bytes -> same artifact bytes (organelle)".to_string(),
        contracts: vec![
            Contract {
                op: "build".to_string(),
                description: "compile a spec into an artifact (deterministic)".to_string(),
                input_schema: serde_json::json!({"type":"object","required":["kind","spec"],"properties":{"kind":{"type":"string"},"spec":{"type":"object"}}}),
                mutation_class: None,
                required_capability: None,
            },
            Contract {
                op: "verify".to_string(),
                description: "verify an artifact's content id against its spec digest".to_string(),
                input_schema: serde_json::json!({"type":"object","required":["artifact"],"properties":{"artifact":{"type":"object"}}}),
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

pub struct PswForgeOrg {
    manifest: OrganelleManifest,
    inner: Forge,
}

pub fn psw_forge_organelle() -> Arc<dyn Organelle> {
    Arc::new(PswForgeOrg {
        manifest: psw_forge_manifest(),
            inner: Forge,
    })
}

impl Organelle for PswForgeOrg {
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
            "build" => {
                let a = self.inner.build(req(&input, "kind")?, &input["spec"])
                                    .map_err(|e| OrganelleError::failed(format!("build failed: {e}")))?;
                                serde_json::to_value(&a)
                                    .map_err(|e| OrganelleError::failed(format!("artifact not JSON: {e}")))
            },
            "verify" => {
                let artifact: crate::Artifact = serde_json::from_value(input["artifact"].clone())
                                    .map_err(|e| OrganelleError::failed(format!("bad artifact: {e}")))?;
                                Ok(serde_json::json!({"valid": self.inner.verify(&artifact)}))
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
    fn psw_forge_static_manifest_validates() {
        let v: Value = serde_json::from_str(include_str!("../../../organelles/psw-forge.manifest.json")).expect("manifest JSON parses");
        let m = validate_manifest_json(&v).expect("static manifest must validate");
        assert_eq!(m.name, "psw-forge");
        assert_eq!(m.codon, "fg");
    }

    #[test]
    fn psw_forge_rust_manifest_matches_static_json() {
        let from_rust = serde_json::to_value(psw_forge_manifest()).unwrap();
        let from_file: Value = serde_json::from_str(include_str!("../../../organelles/psw-forge.manifest.json")).unwrap();
        assert_eq!(from_rust, from_file, "Rust manifest() and organelles/*.json must agree");
    }

    #[test]
    fn psw_forge_registers_and_serves_through_host_with_receipts() {
        let host = Host::new();
        host.register(psw_forge_organelle()).expect("registration must succeed");

        let ctx = CallContext::new("migration-test", vec![]);
        let out = host
            .call(&ctx, "fg", "build", serde_json::json!({"kind": "svg", "spec": {"w": 10}}))
            .expect("build call");
        assert!(out.get("content_id").is_some());
        let out2 = host.call(&ctx, "fg", "build",
            serde_json::json!({"kind": "svg", "spec": {"w": 10}})).expect("rebuild");
        assert_eq!(out.get("content_id"), out2.get("content_id"), "forge must be deterministic");
        let n = host.verify_chain().expect("receipt chain must verify");
        assert!(n >= 2, "register + calls = >=2 receipts, got {n}");
    }
}
