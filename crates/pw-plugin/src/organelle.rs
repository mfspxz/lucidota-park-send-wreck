//! pw-plugin as an organelle (Tier A native migration). Manifest-driven plugin loading.
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
use crate::PluginLoader;
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


fn pw_plugin_manifest() -> OrganelleManifest {
    OrganelleManifest {
        schema_version: MANIFEST_SCHEMA_VERSION.to_string(),
        name: "pw-plugin".to_string(),
        codon: "qp".to_string(),
        version: "0.1.0".to_string(),
        description: "plugin loading v1: manifest-driven, in-process (no untrusted native code) (organelle)".to_string(),
        contracts: vec![
            Contract {
                op: "discover".to_string(),
                description: "discover plugin manifests under a root dir".to_string(),
                input_schema: serde_json::json!({"type":"object","required":["root"],"properties":{"root":{"type":"string"}}}),
                mutation_class: Some(MutationClass::ExternalEffect),
                required_capability: Some("psw.plugin.load".to_string()),
            },
            Contract {
                op: "load_all".to_string(),
                description: "instantiate all discovered plugins via registered factories; returns count".to_string(),
                input_schema: serde_json::json!({"type":"object","required":["root"],"properties":{"root":{"type":"string"}}}),
                mutation_class: Some(MutationClass::ExternalEffect),
                required_capability: Some("psw.plugin.load".to_string()),
            },
        ],
        authority: Authority {
            mutation_class: MutationClass::ExternalEffect,
            declared_valence: Valence::Candidate,
            required_capability: Some("psw.plugin.load".to_string()),
            granted_capabilities: vec!["psw.plugin.load".to_string()],
        },
        transports: vec![Transport::Native, Transport::UnixSocket],
        receipt_policy: ReceiptPolicy {
            content_addressed: true,
            algorithm: RECEIPT_ALGORITHM.to_string(),
            chain: true,
        },
    }
}

pub struct PwPluginOrg {
    manifest: OrganelleManifest,
    inner: PluginLoader,
}

pub fn pw_plugin_organelle() -> Arc<dyn Organelle> {
    Arc::new(PwPluginOrg {
        manifest: pw_plugin_manifest(),
            inner: PluginLoader::default(),
    })
}

impl Organelle for PwPluginOrg {
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
            "discover" => {
                let inner = &self.inner;
                                let plugins = inner.discover(&PathBuf::from(req(&input, "root")?))
                                    .map_err(|e| OrganelleError::failed(format!("discover failed: {e}")))?;
                                let names: Vec<String> = plugins.iter().map(|p| p.dir.to_string_lossy().to_string()).collect();
                                Ok(serde_json::json!({"plugins": names}))
            },
            "load_all" => {
                let v = self.inner.load_all(&PathBuf::from(req(&input, "root")?))
                                    .map_err(|e| OrganelleError::failed(format!("load_all failed: {e}")))?;
                                Ok(serde_json::json!({"loaded": v.len()}))
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
    fn pw_plugin_static_manifest_validates() {
        let v: Value = serde_json::from_str(include_str!("../../../organelles/pw-plugin.manifest.json")).expect("manifest JSON parses");
        let m = validate_manifest_json(&v).expect("static manifest must validate");
        assert_eq!(m.name, "pw-plugin");
        assert_eq!(m.codon, "qp");
    }

    #[test]
    fn pw_plugin_rust_manifest_matches_static_json() {
        let from_rust = serde_json::to_value(pw_plugin_manifest()).unwrap();
        let from_file: Value = serde_json::from_str(include_str!("../../../organelles/pw-plugin.manifest.json")).unwrap();
        assert_eq!(from_rust, from_file, "Rust manifest() and organelles/*.json must agree");
    }

    #[test]
    fn pw_plugin_registers_and_serves_through_host_with_receipts() {
        let host = Host::new();
        host.register(pw_plugin_organelle()).expect("registration must succeed");

        let ctx = CallContext::new("migration-test", vec!["psw.plugin.load".to_string()]);
        let dir = std::env::temp_dir().join("pw-plugin-organelle-test");
        let _ = std::fs::create_dir_all(&dir);
        let out = host.call(&ctx, "qp", "discover",
            serde_json::json!({"root": dir.to_string_lossy()})).expect("discover");
        assert_eq!(out.get("plugins").and_then(|v| v.as_array()).map(|a| a.len()), Some(0));
        let _ = std::fs::remove_dir_all(&dir);
        let n = host.verify_chain().expect("receipt chain must verify");
        assert!(n >= 2, "register + calls = >=2 receipts, got {n}");
    }
}
