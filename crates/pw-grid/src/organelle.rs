//! pw-grid as an organelle (Tier A native migration). The RTS/GIS interaction surface.
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
use pw_domain::{AuthorityTier, EntityId};
use crate::{Grid, GridEntity, SelectionBox};

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

fn parse_tier(s: &str) -> Result<AuthorityTier, OrganelleError> {
    match s {
        "open" => Ok(AuthorityTier::Open),
        "operator" => Ok(AuthorityTier::Operator),
        "admin" => Ok(AuthorityTier::Admin),
        other => Err(OrganelleError::failed(format!("unknown tier '{other}' (open|operator|admin)"))),
    }
}


fn pw_grid_manifest() -> OrganelleManifest {
    OrganelleManifest {
        schema_version: MANIFEST_SCHEMA_VERSION.to_string(),
        name: "pw-grid".to_string(),
        codon: "gr".to_string(),
        version: "0.1.0".to_string(),
        description: "RTS/GIS interaction surface: entities on a 2D grid, box selection (organelle)".to_string(),
        contracts: vec![
            Contract {
                op: "insert".to_string(),
                description: "insert a grid entity (quarantined entities never enter selections)".to_string(),
                input_schema: serde_json::json!({"type":"object","required":["id","x","y"],"properties":{"id":{"type":"string"},"x":{"type":"number"},"y":{"type":"number"},"tier":{"type":"string"},"quarantined":{"type":"boolean"}}}),
                mutation_class: Some(MutationClass::Write),
                required_capability: Some("psw.grid.write".to_string()),
            },
            Contract {
                op: "query_box".to_string(),
                description: "entities inside a selection box".to_string(),
                input_schema: serde_json::json!({"type":"object","required":["x0","y0","x1","y1"],"properties":{"x0":{"type":"number"},"y0":{"type":"number"},"x1":{"type":"number"},"y1":{"type":"number"}}}),
                mutation_class: None,
                required_capability: None,
            },
            Contract {
                op: "len".to_string(),
                description: "entity count".to_string(),
                input_schema: serde_json::json!({"type":"object"}),
                mutation_class: None,
                required_capability: None,
            },
        ],
        authority: Authority {
            mutation_class: MutationClass::Write,
            declared_valence: Valence::Candidate,
            required_capability: Some("psw.grid.write".to_string()),
            granted_capabilities: vec!["psw.grid.write".to_string()],
        },
        transports: vec![Transport::Native, Transport::UnixSocket],
        receipt_policy: ReceiptPolicy {
            content_addressed: true,
            algorithm: RECEIPT_ALGORITHM.to_string(),
            chain: true,
        },
    }
}

pub struct PwGridOrg {
    manifest: OrganelleManifest,
    inner: Mutex<Grid>,
}

pub fn pw_grid_organelle() -> Arc<dyn Organelle> {
    Arc::new(PwGridOrg {
        manifest: pw_grid_manifest(),
            inner: Mutex::new(Grid::new()),
    })
}

impl Organelle for PwGridOrg {
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
            "insert" => {
                let e = GridEntity {
                                    id: EntityId(req(&input, "id")?.to_string()),
                                    x: input.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0),
                                    y: input.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0),
                                    tier: parse_tier(opt_str(&input, "tier").unwrap_or_else(|| "open".to_string()).as_str())?,
                                    quarantined: input.get("quarantined").and_then(|v| v.as_bool()).unwrap_or(false),
                                };
                                let mut inner = self.inner.lock().unwrap();
                                inner.insert(e);
                                Ok(serde_json::json!({"ok": true}))
            },
            "query_box" => {
                let b = SelectionBox::new(
                                    input.get("x0").and_then(|v| v.as_f64()).unwrap_or(0.0),
                                    input.get("y0").and_then(|v| v.as_f64()).unwrap_or(0.0),
                                    input.get("x1").and_then(|v| v.as_f64()).unwrap_or(0.0),
                                    input.get("y1").and_then(|v| v.as_f64()).unwrap_or(0.0));
                                let inner = self.inner.lock().unwrap();
                                let ids: Vec<String> = inner.query_box(&b).iter().map(|e| e.id.0.clone()).collect();
                                Ok(serde_json::json!({"entities": ids}))
            },
            "len" => {
                let inner = self.inner.lock().unwrap();
                                Ok(serde_json::json!({"len": inner.len()}))
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
    fn pw_grid_static_manifest_validates() {
        let v: Value = serde_json::from_str(include_str!("../../../organelles/pw-grid.manifest.json")).expect("manifest JSON parses");
        let m = validate_manifest_json(&v).expect("static manifest must validate");
        assert_eq!(m.name, "pw-grid");
        assert_eq!(m.codon, "gr");
    }

    #[test]
    fn pw_grid_rust_manifest_matches_static_json() {
        let from_rust = serde_json::to_value(pw_grid_manifest()).unwrap();
        let from_file: Value = serde_json::from_str(include_str!("../../../organelles/pw-grid.manifest.json")).unwrap();
        assert_eq!(from_rust, from_file, "Rust manifest() and organelles/*.json must agree");
    }

    #[test]
    fn pw_grid_registers_and_serves_through_host_with_receipts() {
        let host = Host::new();
        host.register(pw_grid_organelle()).expect("registration must succeed");

        let ctx = CallContext::new("migration-test", vec!["psw.grid.write".to_string()]);
        let out = host
            .call(&ctx, "gr", "insert", serde_json::json!({"id": "e1", "x": 1.0, "y": 1.0, "tier": "open"}))
            .expect("insert call");
        assert_eq!(out.get("ok").and_then(|v| v.as_bool()), Some(true));
        let out = host
            .call(&ctx, "gr", "query_box", serde_json::json!({"x0": 0.0, "y0": 0.0, "x1": 5.0, "y1": 5.0}))
            .expect("query_box call");
        assert_eq!(out.get("entities").and_then(|v| v.as_array()).map(|a| a.len()), Some(1));
        let n = host.verify_chain().expect("receipt chain must verify");
        assert!(n >= 2, "register + calls = >=2 receipts, got {n}");
    }
}
