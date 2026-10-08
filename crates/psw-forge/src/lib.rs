//! psw-003 — Forge: deterministic artifact builder.
//! A spec (JSON) is compiled into an artifact. Same spec bytes -> same
//! artifact bytes -> same blake2 content id. Determinism is the contract:
//! a forge that builds different bytes from the same spec is broken.
use pw_domain::{hash_bytes_hex, AuthorityTier};
use pw_manifest::{Contract, OrganelleManifest, ReceiptPolicy, Transport, SCHEMA_VERSION};
use pw_organelles::{Organelle, OrganelleError};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifact {
    pub content_id: String,
    pub kind: String,
    pub bytes: Vec<u8>,
    pub spec_digest: String,
}

pub struct Forge;

impl Forge {
    /// Deterministic build: canonical JSON spec -> bytes.
    /// v1 compiler: sorts keys, emits a simple framed binary:
    /// `PSW1/<kind>/<canonical-spec-json>`.
    pub fn build(&self, kind: &str, spec: &Value) -> Result<Artifact, OrganelleError> {
        if kind.trim().is_empty() {
            return Err(OrganelleError::BadInput("empty kind".into()));
        }
        let canon = canonical_json(spec);
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"PSW1/");
        bytes.extend_from_slice(kind.as_bytes());
        bytes.push(b'/');
        bytes.extend_from_slice(canon.as_bytes());
        let content_id = hash_bytes_hex(&bytes);
        let spec_digest = hash_bytes_hex(canon.as_bytes());
        Ok(Artifact { content_id, kind: kind.into(), bytes, spec_digest })
    }

    pub fn verify(&self, artifact: &Artifact) -> bool {
        artifact.content_id == hash_bytes_hex(&artifact.bytes)
    }
}

fn canonical_json(v: &Value) -> String {
    match v {
        Value::Object(m) => {
            let mut keys: Vec<&String> = m.keys().collect();
            keys.sort();
            let parts: Vec<String> = keys.iter().map(|k| format!("{}:{}", serde_json::to_string(k).unwrap(), canonical_json(&m[*k]))).collect();
            format!("{{{}}}", parts.join(","))
        }
        Value::Array(a) => format!("[{}]", a.iter().map(canonical_json).collect::<Vec<_>>().join(",")),
        _ => serde_json::to_string(v).unwrap(),
    }
}

fn manifest() -> OrganelleManifest {
    OrganelleManifest {
        schema: SCHEMA_VERSION.into(),
        name: "psw-forge".into(),
        version: "0.1.0".into(),
        description: "Deterministic artifact builder: spec -> content-addressed artifact".into(),
        contracts: vec![
            Contract { name: "build".into(), input_schema: "{\"kind\":str,\"spec\":json}".into(), output_schema: "Artifact".into(), idempotent: true },
            Contract { name: "verify".into(), input_schema: "Artifact".into(), output_schema: "{\"valid\":bool}".into(), idempotent: true },
        ],
        authority: AuthorityTier::Operator,
        transports: vec![Transport::InProcess],
        receipts: ReceiptPolicy::Always,
        entrypoint: "psw_forge::Forge".into(),
    }
}

impl Organelle for Forge {
    fn name(&self) -> &'static str {
        "psw-forge"
    }
    fn manifest(&self) -> OrganelleManifest {
        manifest()
    }
    fn call(&self, contract: &str, input: Value) -> Result<Value, OrganelleError> {
        match contract {
            "build" => {
                let kind = input.get("kind").and_then(|v| v.as_str()).ok_or_else(|| OrganelleError::BadInput("kind".into()))?;
                let spec = input.get("spec").cloned().unwrap_or(Value::Null);
                Ok(serde_json::to_value(self.build(kind, &spec)?).unwrap())
            }
            "verify" => {
                let a: Artifact = serde_json::from_value(input).map_err(|e| OrganelleError::BadInput(e.to_string()))?;
                Ok(serde_json::json!({"valid": self.verify(&a)}))
            }
            _ => Err(OrganelleError::UnknownContract(contract.into(), self.name().into())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_build() {
        let f = Forge;
        let spec = serde_json::json!({"b": 1, "a": [1, 2]});
        let a1 = f.build("test", &spec).unwrap();
        let a2 = f.build("test", &spec).unwrap();
        assert_eq!(a1.content_id, a2.content_id);
        assert_eq!(a1.bytes, a2.bytes);
    }

    #[test]
    fn key_order_does_not_matter() {
        let f = Forge;
        let a1 = f.build("k", &serde_json::json!({"a": 1, "b": 2})).unwrap();
        let a2 = f.build("k", &serde_json::json!({"b": 2, "a": 1})).unwrap();
        assert_eq!(a1.content_id, a2.content_id);
    }

    #[test]
    fn different_spec_different_id() {
        let f = Forge;
        let a1 = f.build("k", &serde_json::json!({"a": 1})).unwrap();
        let a2 = f.build("k", &serde_json::json!({"a": 2})).unwrap();
        assert_ne!(a1.content_id, a2.content_id);
    }

    #[test]
    fn different_kind_different_bytes() {
        let f = Forge;
        let a1 = f.build("k1", &serde_json::json!({})).unwrap();
        let a2 = f.build("k2", &serde_json::json!({})).unwrap();
        assert_ne!(a1.content_id, a2.content_id);
    }

    #[test]
    fn verify_fresh_artifact() {
        let f = Forge;
        let a = f.build("k", &serde_json::json!({"x": true})).unwrap();
        assert!(f.verify(&a));
    }

    #[test]
    fn verify_detects_tamper() {
        let f = Forge;
        let mut a = f.build("k", &serde_json::json!({"x": true})).unwrap();
        a.bytes.push(0xFF);
        assert!(!f.verify(&a));
    }

    #[test]
    fn empty_kind_rejected() {
        let f = Forge;
        assert!(f.build("", &Value::Null).is_err());
    }

    #[test]
    fn organelle_build_contract() {
        let f = Forge;
        let out = f.call("build", serde_json::json!({"kind": "k", "spec": {"a": 1}})).unwrap();
        assert!(out["content_id"].as_str().unwrap().len() == 128);
        let valid = f.call("verify", out).unwrap();
        assert_eq!(valid["valid"], true);
    }
}
