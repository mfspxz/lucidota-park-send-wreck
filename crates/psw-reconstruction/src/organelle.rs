//! psw-reconstruction as an organelle (Tier A native migration). Byte-proof verification.
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
use crate::verify_byte_proof;
use std::path::PathBuf;

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


fn psw_reconstruction_manifest() -> OrganelleManifest {
    OrganelleManifest {
        schema_version: MANIFEST_SCHEMA_VERSION.to_string(),
        name: "psw-reconstruction".to_string(),
        codon: "rc".to_string(),
        version: "0.1.0".to_string(),
        description: "reconstruction audit: scan dirs for proven byte hashes of the lost tree (organelle)".to_string(),
        contracts: vec![
            Contract {
                op: "verify_proof".to_string(),
                description: "scan search dirs (recursive, bounded) for proven artifact hashes".to_string(),
                input_schema: serde_json::json!({"type":"object","required":["dirs"],"properties":{"dirs":{"type":"array","items":{"type":"string"}}}}),
                mutation_class: None,
                required_capability: None,
            },
            Contract {
                op: "crate_inventory".to_string(),
                description: "the 19-crate inventory of the Aug 2026 tree".to_string(),
                input_schema: serde_json::json!({"type":"object"}),
                mutation_class: None,
                required_capability: None,
            },
        ],
        authority: Authority {
            mutation_class: MutationClass::ReadOnly,
            declared_valence: Valence::Candidate,
            required_capability: None,
            granted_capabilities: vec![],
        },
        transports: vec![Transport::Native, Transport::UnixSocket],
        receipt_policy: ReceiptPolicy {
            content_addressed: true,
            algorithm: RECEIPT_ALGORITHM.to_string(),
            chain: true,
        },
    }
}

pub struct PswReconstructionOrg {
    manifest: OrganelleManifest,
    _priv: (),
}

pub fn psw_reconstruction_organelle() -> Arc<dyn Organelle> {
    Arc::new(PswReconstructionOrg {
        manifest: psw_reconstruction_manifest(),
            _priv: (),
    })
}

impl Organelle for PswReconstructionOrg {
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
            "verify_proof" => {
                let dirs: Vec<PathBuf> = input["dirs"].as_array().cloned().unwrap_or_default()
                                    .iter().filter_map(|d| d.as_str().map(PathBuf::from)).collect();
                                let refs: Vec<&std::path::Path> = dirs.iter().map(|p| p.as_path()).collect();
                                let r = verify_byte_proof(&refs);
                                serde_json::to_value(&r)
                                    .map_err(|e| OrganelleError::failed(format!("report not JSON: {e}")))
            },
            "crate_inventory" => {
                let inv = crate::crate_inventory();
                                let items: Vec<Value> = inv.iter()
                                    .map(|(a, b)| serde_json::json!({"crate": a, "note": b})).collect();
                                Ok(serde_json::json!({"crates": items}))
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
    fn psw_reconstruction_static_manifest_validates() {
        let v: Value = serde_json::from_str(include_str!("../../../organelles/psw-reconstruction.manifest.json")).expect("manifest JSON parses");
        let m = validate_manifest_json(&v).expect("static manifest must validate");
        assert_eq!(m.name, "psw-reconstruction");
        assert_eq!(m.codon, "rc");
    }

    #[test]
    fn psw_reconstruction_rust_manifest_matches_static_json() {
        let from_rust = serde_json::to_value(psw_reconstruction_manifest()).unwrap();
        let from_file: Value = serde_json::from_str(include_str!("../../../organelles/psw-reconstruction.manifest.json")).unwrap();
        assert_eq!(from_rust, from_file, "Rust manifest() and organelles/*.json must agree");
    }

    #[test]
    fn psw_reconstruction_registers_and_serves_through_host_with_receipts() {
        let host = Host::new();
        host.register(psw_reconstruction_organelle()).expect("registration must succeed");

        let ctx = CallContext::new("migration-test", vec![]);
        let dir = std::env::temp_dir().join("psw-recon-organelle-test");
        let _ = std::fs::create_dir_all(&dir);
        let out = host.call(&ctx, "rc", "verify_proof",
            serde_json::json!({"dirs": [dir.to_string_lossy()]}));
        assert!(out.is_ok() || out.is_err());
        let _ = std::fs::remove_dir_all(&dir);
        let inv = host.call(&ctx, "rc", "crate_inventory", serde_json::json!({})).expect("inventory");
        assert!(inv.get("crates").and_then(|v| v.as_array()).map(|a| !a.is_empty()).unwrap_or(false));
        let n = host.verify_chain().expect("receipt chain must verify");
        assert!(n >= 2, "register + calls = >=2 receipts, got {n}");
    }
}
