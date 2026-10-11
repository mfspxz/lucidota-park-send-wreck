//! psw-court as an organelle (Tier A native migration). Claim adjudication.
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
use crate::{Court, Evidence, EvidenceStance};

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

fn parse_stance(s: &str) -> Result<EvidenceStance, OrganelleError> {
    match s {
        "supports" => Ok(EvidenceStance::Supports),
        "refutes" => Ok(EvidenceStance::Refutes),
        "neutral" => Ok(EvidenceStance::Neutral),
        other => Err(OrganelleError::failed(format!("unknown stance '{other}' (supports|refutes|neutral)"))),
    }
}


fn psw_court_manifest() -> OrganelleManifest {
    OrganelleManifest {
        schema_version: MANIFEST_SCHEMA_VERSION.to_string(),
        name: "psw-court".to_string(),
        codon: "qt".to_string(),
        version: "0.1.0".to_string(),
        description: "adjudication of claims: evidence tagged supports/refutes/neutral -> ruling (organelle)".to_string(),
        contracts: vec![
            Contract {
                op: "file".to_string(),
                description: "file a claim with evidence items; returns the claim id".to_string(),
                input_schema: serde_json::json!({"type":"object","required":["statement","evidence"],"properties":{"statement":{"type":"string"},"evidence":{"type":"array"}}}),
                mutation_class: Some(MutationClass::Write),
                required_capability: Some("psw.court.write".to_string()),
            },
            Contract {
                op: "adjudicate".to_string(),
                description: "rule on a filed claim".to_string(),
                input_schema: serde_json::json!({"type":"object","required":["claim_id"],"properties":{"claim_id":{"type":"string"}}}),
                mutation_class: Some(MutationClass::Write),
                required_capability: Some("psw.court.write".to_string()),
            },
            Contract {
                op: "rulings_for".to_string(),
                description: "rulings recorded for a claim".to_string(),
                input_schema: serde_json::json!({"type":"object","required":["claim_id"],"properties":{"claim_id":{"type":"string"}}}),
                mutation_class: None,
                required_capability: None,
            },
        ],
        authority: Authority {
            mutation_class: MutationClass::Write,
            declared_valence: Valence::Candidate,
            required_capability: Some("psw.court.write".to_string()),
            granted_capabilities: vec!["psw.court.write".to_string()],
        },
        transports: vec![Transport::Native, Transport::UnixSocket],
        receipt_policy: ReceiptPolicy {
            content_addressed: true,
            algorithm: RECEIPT_ALGORITHM.to_string(),
            chain: true,
        },
    }
}

pub struct PswCourtOrg {
    manifest: OrganelleManifest,
    inner: Mutex<Court>,
}

pub fn psw_court_organelle() -> Arc<dyn Organelle> {
    Arc::new(PswCourtOrg {
        manifest: psw_court_manifest(),
            inner: Mutex::new(Court::new()),
    })
}

impl Organelle for PswCourtOrg {
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
            "file" => {
                let ev: Vec<Evidence> = input["evidence"].as_array().cloned().unwrap_or_default()
                                    .iter().map(|e| Ok(Evidence {
                                        id: e.get("id").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                                        stance: parse_stance(e.get("stance").and_then(|x| x.as_str()).unwrap_or("neutral"))?,
                                        summary: e.get("summary").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                                    })).collect::<Result<_, OrganelleError>>()?;
                                let inner = self.inner.lock().unwrap();
                                let id = inner.file(req(&input, "statement")?, ev);
                                Ok(serde_json::json!({"claim_id": id}))
            },
            "adjudicate" => {
                let inner = self.inner.lock().unwrap();
                                let r = inner.adjudicate(req(&input, "claim_id")?)
                                    .map_err(|e| OrganelleError::failed(format!("adjudicate failed: {e}")))?;
                                serde_json::to_value(&r)
                                    .map_err(|e| OrganelleError::failed(format!("ruling not JSON: {e}")))
            },
            "rulings_for" => {
                let inner = self.inner.lock().unwrap();
                                let v = inner.rulings_for(req(&input, "claim_id")?);
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
    fn psw_court_static_manifest_validates() {
        let v: Value = serde_json::from_str(include_str!("../../../organelles/psw-court.manifest.json")).expect("manifest JSON parses");
        let m = validate_manifest_json(&v).expect("static manifest must validate");
        assert_eq!(m.name, "psw-court");
        assert_eq!(m.codon, "qt");
    }

    #[test]
    fn psw_court_rust_manifest_matches_static_json() {
        let from_rust = serde_json::to_value(psw_court_manifest()).unwrap();
        let from_file: Value = serde_json::from_str(include_str!("../../../organelles/psw-court.manifest.json")).unwrap();
        assert_eq!(from_rust, from_file, "Rust manifest() and organelles/*.json must agree");
    }

    #[test]
    fn psw_court_registers_and_serves_through_host_with_receipts() {
        let host = Host::new();
        host.register(psw_court_organelle()).expect("registration must succeed");

        let ctx = CallContext::new("migration-test", vec!["psw.court.write".to_string()]);
        let out = host.call(&ctx, "qt", "file", serde_json::json!(
            {"statement": "the build is green",
             "evidence": [{"id": "e1", "stance": "supports", "summary": "ci log"},
                          {"id": "e2", "stance": "refutes", "summary": "flaky test"}]})).expect("file");
        let cid = out.get("claim_id").and_then(|v| v.as_str()).unwrap().to_string();
        let ruling = host.call(&ctx, "qt", "adjudicate",
            serde_json::json!({"claim_id": cid})).expect("adjudicate");
        assert!(ruling.get("verdict").is_some());
        assert!(ruling.get("rationale").is_some());
        let n = host.verify_chain().expect("receipt chain must verify");
        assert!(n >= 2, "register + calls = >=2 receipts, got {n}");
    }
}
