//! psw-parity as an organelle (Tier A native migration). Transport-equivalence checker.
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
use crate::{Parity, TransportSample};

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


fn psw_parity_manifest() -> OrganelleManifest {
    OrganelleManifest {
        schema_version: MANIFEST_SCHEMA_VERSION.to_string(),
        name: "psw-parity".to_string(),
        codon: "py".to_string(),
        version: "0.1.0".to_string(),
        description: "transport-equivalence checker: same organelle+contract across transports must agree (organelle)".to_string(),
        contracts: vec![
            Contract {
                op: "check".to_string(),
                description: "check transport samples for equivalence (digests must all match)".to_string(),
                input_schema: serde_json::json!({"type":"object","required":["organelle","contract","samples"],"properties":{"organelle":{"type":"string"},"contract":{"type":"string"},"samples":{"type":"array"}}}),
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
        transports: vec![Transport::Native, Transport::UnixSocket],
        receipt_policy: ReceiptPolicy {
            content_addressed: true,
            algorithm: RECEIPT_ALGORITHM.to_string(),
            chain: true,
        },
    }
}

pub struct PswParityOrg {
    manifest: OrganelleManifest,
    _priv: (),
}

pub fn psw_parity_organelle() -> Arc<dyn Organelle> {
    Arc::new(PswParityOrg {
        manifest: psw_parity_manifest(),
            _priv: (),
    })
}

impl Organelle for PswParityOrg {
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
            "check" => {
                let samples: Vec<TransportSample> = input["samples"].as_array().cloned().unwrap_or_default()
                                    .iter().map(|s| TransportSample {
                                        transport: s.get("transport").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                                        output_digest: s.get("output_digest").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                                    }).collect();
                                let r = Parity::check(req(&input, "organelle")?, req(&input, "contract")?, &samples);
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
    fn psw_parity_static_manifest_validates() {
        let v: Value = serde_json::from_str(include_str!("../../../organelles/psw-parity.manifest.json")).expect("manifest JSON parses");
        let m = validate_manifest_json(&v).expect("static manifest must validate");
        assert_eq!(m.name, "psw-parity");
        assert_eq!(m.codon, "py");
    }

    #[test]
    fn psw_parity_rust_manifest_matches_static_json() {
        let from_rust = serde_json::to_value(psw_parity_manifest()).unwrap();
        let from_file: Value = serde_json::from_str(include_str!("../../../organelles/psw-parity.manifest.json")).unwrap();
        assert_eq!(from_rust, from_file, "Rust manifest() and organelles/*.json must agree");
    }

    #[test]
    fn psw_parity_registers_and_serves_through_host_with_receipts() {
        let host = Host::new();
        host.register(psw_parity_organelle()).expect("registration must succeed");

        let ctx = CallContext::new("migration-test", vec![]);
        let out = host
            .call(&ctx, "py", "check", serde_json::json!({"organelle": "psw-forge", "contract": "build", "samples": [{"transport": "native", "output_digest": "aa"}, {"transport": "rest", "output_digest": "aa"}]}))
            .expect("check call");
        assert_eq!(out.get("equivalent").and_then(|v| v.as_bool()), Some(true));
        let out = host
            .call(&ctx, "py", "check", serde_json::json!({"organelle": "psw-forge", "contract": "build", "samples": [{"transport": "native", "output_digest": "aa"}, {"transport": "rest", "output_digest": "bb"}]}))
            .expect("check call");
        assert_eq!(out.get("equivalent").and_then(|v| v.as_bool()), Some(false));
        let n = host.verify_chain().expect("receipt chain must verify");
        assert!(n >= 2, "register + calls = >=2 receipts, got {n}");
    }
}
