//! psw-arts-crafts as an organelle (Tier A native migration). Grid state -> SVG maps.
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
use crate::{ArtsCrafts, MapEntity};

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


fn psw_arts_crafts_manifest() -> OrganelleManifest {
    OrganelleManifest {
        schema_version: MANIFEST_SCHEMA_VERSION.to_string(),
        name: "psw-arts-crafts".to_string(),
        codon: "ac".to_string(),
        version: "0.1.0".to_string(),
        description: "rendering organelle: grid state + selections -> SVG maps (organelle)".to_string(),
        contracts: vec![
            Contract {
                op: "render_map".to_string(),
                description: "render entities (+ optional selection box) to an SVG map".to_string(),
                input_schema: serde_json::json!({"type":"object","required":["entities"],"properties":{"entities":{"type":"array"},"sel_box":{"type":"array","items":{"type":"number"},"minItems":4,"maxItems":4}}}),
                mutation_class: None,
                required_capability: None,
            },
            Contract {
                op: "render_receipt_count".to_string(),
                description: "render a receipt-count badge SVG".to_string(),
                input_schema: serde_json::json!({"type":"object","required":["count"],"properties":{"count":{"type":"integer"}}}),
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

pub struct PswArtsCraftsOrg {
    manifest: OrganelleManifest,
    inner: ArtsCrafts,
}

pub fn psw_arts_crafts_organelle() -> Arc<dyn Organelle> {
    Arc::new(PswArtsCraftsOrg {
        manifest: psw_arts_crafts_manifest(),
            inner: ArtsCrafts,
    })
}

impl Organelle for PswArtsCraftsOrg {
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
            "render_map" => {
                let entities: Vec<MapEntity> = input["entities"].as_array().cloned().unwrap_or_default()
                                    .iter().map(|e| MapEntity {
                                        id: e.get("id").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                                        x: e.get("x").and_then(|x| x.as_f64()).unwrap_or(0.0),
                                        y: e.get("y").and_then(|x| x.as_f64()).unwrap_or(0.0),
                                        quarantined: e.get("quarantined").and_then(|x| x.as_bool()).unwrap_or(false),
                                        selected: e.get("selected").and_then(|x| x.as_bool()).unwrap_or(false),
                                    }).collect();
                                let sel: Option<(f64, f64, f64, f64)> = input.get("sel_box").and_then(|v| v.as_array())
                                    .and_then(|a| {
                                        Some((a.get(0)?.as_f64()?, a.get(1)?.as_f64()?,
                                               a.get(2)?.as_f64()?, a.get(3)?.as_f64()?))
                                    });
                                let svg = self.inner.render_map(&entities, sel);
                                Ok(serde_json::json!({"svg": svg}))
            },
            "render_receipt_count" => {
                let svg = self.inner.render_receipt_count(
                                    input.get("count").and_then(|v| v.as_u64()).unwrap_or(0) as usize);
                                Ok(serde_json::json!({"svg": svg}))
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
    fn psw_arts_crafts_static_manifest_validates() {
        let v: Value = serde_json::from_str(include_str!("../../../organelles/psw-arts-crafts.manifest.json")).expect("manifest JSON parses");
        let m = validate_manifest_json(&v).expect("static manifest must validate");
        assert_eq!(m.name, "psw-arts-crafts");
        assert_eq!(m.codon, "ac");
    }

    #[test]
    fn psw_arts_crafts_rust_manifest_matches_static_json() {
        let from_rust = serde_json::to_value(psw_arts_crafts_manifest()).unwrap();
        let from_file: Value = serde_json::from_str(include_str!("../../../organelles/psw-arts-crafts.manifest.json")).unwrap();
        assert_eq!(from_rust, from_file, "Rust manifest() and organelles/*.json must agree");
    }

    #[test]
    fn psw_arts_crafts_registers_and_serves_through_host_with_receipts() {
        let host = Host::new();
        host.register(psw_arts_crafts_organelle()).expect("registration must succeed");

        let ctx = CallContext::new("migration-test", vec![]);
        let out = host
            .call(&ctx, "ac", "render_map", serde_json::json!({"entities": [{"id": "e1", "x": 1.0, "y": 2.0, "quarantined": false, "selected": true}]}))
            .expect("render_map call");
        assert!(out.get("svg").and_then(|v| v.as_str()).map(|s| s.contains("<svg")).unwrap_or(false));
        let n = host.verify_chain().expect("receipt chain must verify");
        assert!(n >= 2, "register + calls = >=2 receipts, got {n}");
    }
}
