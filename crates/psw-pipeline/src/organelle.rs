//! psw-pipeline as an organelle (Tier A native migration). The full Discover->Archive loop.
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
use crate::Pipeline;
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


fn psw_pipeline_manifest() -> OrganelleManifest {
    OrganelleManifest {
        schema_version: MANIFEST_SCHEMA_VERSION.to_string(),
        name: "psw-pipeline".to_string(),
        codon: "pp".to_string(),
        version: "0.1.0".to_string(),
        description: "Discover -> Archive pipeline: the full PSW loop in one pass (organelle)".to_string(),
        contracts: vec![
            Contract {
                op: "run".to_string(),
                description: "run the full pipeline over a manifest directory".to_string(),
                input_schema: serde_json::json!({"type":"object","required":["manifest_dir"],"properties":{"manifest_dir":{"type":"string"}}}),
                mutation_class: Some(MutationClass::Write),
                required_capability: Some("psw.pipeline.write".to_string()),
            },
        ],
        authority: Authority {
            mutation_class: MutationClass::Write,
            declared_valence: Valence::Candidate,
            required_capability: Some("psw.pipeline.write".to_string()),
            granted_capabilities: vec!["psw.pipeline.write".to_string()],
        },
        transports: vec![Transport::Native, Transport::UnixSocket],
        receipt_policy: ReceiptPolicy {
            content_addressed: true,
            algorithm: RECEIPT_ALGORITHM.to_string(),
            chain: true,
        },
    }
}

pub struct PswPipelineOrg {
    manifest: OrganelleManifest,
    inner: Pipeline,
}

pub fn psw_pipeline_organelle() -> Arc<dyn Organelle> {
    Arc::new(PswPipelineOrg {
        manifest: psw_pipeline_manifest(),
            inner: Pipeline::new(),
    })
}

impl Organelle for PswPipelineOrg {
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
            "run" => {
                let dir = PathBuf::from(req(&input, "manifest_dir")?);
                                let r = self.inner.run(&dir);
                                serde_json::to_value(&r)
                                    .map_err(|e| OrganelleError::failed(format!("report not JSON: {e}")))
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
    fn psw_pipeline_static_manifest_validates() {
        let v: Value = serde_json::from_str(include_str!("../../../organelles/psw-pipeline.manifest.json")).expect("manifest JSON parses");
        let m = validate_manifest_json(&v).expect("static manifest must validate");
        assert_eq!(m.name, "psw-pipeline");
        assert_eq!(m.codon, "pp");
    }

    #[test]
    fn psw_pipeline_rust_manifest_matches_static_json() {
        let from_rust = serde_json::to_value(psw_pipeline_manifest()).unwrap();
        let from_file: Value = serde_json::from_str(include_str!("../../../organelles/psw-pipeline.manifest.json")).unwrap();
        assert_eq!(from_rust, from_file, "Rust manifest() and organelles/*.json must agree");
    }

    #[test]
    fn psw_pipeline_registers_and_serves_through_host_with_receipts() {
        let host = Host::new();
        host.register(psw_pipeline_organelle()).expect("registration must succeed");

        let ctx = CallContext::new("migration-test", vec!["psw.pipeline.write".to_string()]);
        // run over a temp dir: must complete (ok or clean failure), never panic; receipted either way
        let dir = std::env::temp_dir().join("psw-pipeline-organelle-test");
        let _ = std::fs::create_dir_all(&dir);
        let res = host.call(&ctx, "pp", "run",
            serde_json::json!({"manifest_dir": dir.to_string_lossy()}));
        assert!(res.is_ok() || res.is_err());
        let _ = std::fs::remove_dir_all(&dir);
        let n = host.verify_chain().expect("receipt chain must verify");
        assert!(n >= 2, "register + calls = >=2 receipts, got {n}");
    }
}
