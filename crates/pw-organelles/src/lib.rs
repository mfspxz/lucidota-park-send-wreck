//! parkwreck-organelles — proven crate name from the Aug 2026 tree.
//! The Organelle trait: the single contract every PSW subsystem implements.
//! An organelle is invoked through one `call` surface; transports are just
//! different doors to the same room (behavioral equivalence enforced by
//! the Parity subsystem).
use pw_domain::{AuthorityTier, DomainError};
use pw_manifest::{OrganelleManifest, Transport};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum OrganelleError {
    #[error("unknown contract {0:?} on organelle {1:?}")]
    UnknownContract(String, String),
    #[error("bad input: {0}")]
    BadInput(String),
    #[error("execution failed: {0}")]
    Execution(String),
    #[error("domain: {0}")]
    Domain(#[from] DomainError),
}

pub trait Organelle: Send + Sync {
    fn name(&self) -> &'static str;
    fn manifest(&self) -> OrganelleManifest;
    fn authority_required(&self) -> AuthorityTier {
        self.manifest().authority
    }
    /// The single call surface. `contract` must be declared in the manifest.
    fn call(&self, contract: &str, input: Value) -> Result<Value, OrganelleError>;
}

/// In-process registry: name -> organelle implementation.
#[derive(Default)]
pub struct Registry {
    inner: RwLock<HashMap<String, Arc<dyn Organelle>>>,
}

#[derive(Debug, Error)]
pub enum RegistryError {
    #[error("organelle {0:?} already registered")]
    Duplicate(String),
    #[error("manifest invalid for {0:?}: {1}")]
    InvalidManifest(String, String),
    #[error("organelle {0:?} not registered")]
    Unknown(String),
}

impl Registry {
    pub fn new() -> Self {
        Registry::default()
    }

    /// Fail-closed: manifest must validate or registration is refused.
    pub fn register(&self, organelle: Arc<dyn Organelle>) -> Result<(), RegistryError> {
        let m = organelle.manifest();
        m.validate().map_err(|e| RegistryError::InvalidManifest(organelle.name().into(), e.to_string()))?;
        if m.name != organelle.name() {
            return Err(RegistryError::InvalidManifest(
                organelle.name().into(),
                "manifest name != organelle name".into(),
            ));
        }
        let mut w = self.inner.write().unwrap();
        if w.contains_key(&m.name) {
            return Err(RegistryError::Duplicate(m.name));
        }
        w.insert(m.name.clone(), organelle);
        Ok(())
    }

    pub fn get(&self, name: &str) -> Result<Arc<dyn Organelle>, RegistryError> {
        self.inner.read().unwrap().get(name).cloned().ok_or_else(|| RegistryError::Unknown(name.into()))
    }

    pub fn names(&self) -> Vec<String> {
        let mut v: Vec<String> = self.inner.read().unwrap().keys().cloned().collect();
        v.sort();
        v
    }

    pub fn manifests(&self) -> Vec<OrganelleManifest> {
        self.inner.read().unwrap().values().map(|o| o.manifest()).collect()
    }
}

/// Transport label used for parity checks (no real sockets in v1; the
/// transport is recorded on the call path so Parity can compare digests).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallTransport {
    Native(Transport),
}

#[cfg(test)]
mod tests {
    use super::*;
    use pw_manifest::{sample_manifest, ReceiptPolicy};

    struct Echo;
    impl Organelle for Echo {
        fn name(&self) -> &'static str {
            "psw-echo"
        }
        fn manifest(&self) -> OrganelleManifest {
            let mut m = sample_manifest("psw-echo");
            m.receipts = ReceiptPolicy::Always;
            m
        }
        fn call(&self, contract: &str, input: Value) -> Result<Value, OrganelleError> {
            match contract {
                "ping" => Ok(serde_json::json!({"pong": true, "in": input})),
                _ => Err(OrganelleError::UnknownContract(contract.into(), self.name().into())),
            }
        }
    }

    #[test]
    fn register_and_call() {
        let r = Registry::new();
        r.register(Arc::new(Echo)).unwrap();
        let o = r.get("psw-echo").unwrap();
        let out = o.call("ping", serde_json::json!({"a": 1})).unwrap();
        assert_eq!(out["pong"], true);
    }

    #[test]
    fn duplicate_registration_refused() {
        let r = Registry::new();
        r.register(Arc::new(Echo)).unwrap();
        assert!(matches!(r.register(Arc::new(Echo)), Err(RegistryError::Duplicate(_))));
    }

    #[test]
    fn unknown_lookup_refused() {
        let r = Registry::new();
        assert!(matches!(r.get("nope"), Err(RegistryError::Unknown(_))));
    }

    #[test]
    fn unknown_contract_errors() {
        let e = Echo;
        assert!(matches!(e.call("nope", Value::Null), Err(OrganelleError::UnknownContract(_, _))));
    }

    struct BadManifest;
    impl Organelle for BadManifest {
        fn name(&self) -> &'static str {
            "psw-bad"
        }
        fn manifest(&self) -> OrganelleManifest {
            let mut m = sample_manifest("psw-bad");
            m.contracts.clear();
            m
        }
        fn call(&self, _c: &str, _i: Value) -> Result<Value, OrganelleError> {
            Ok(Value::Null)
        }
    }

    #[test]
    fn invalid_manifest_refused_at_register() {
        let r = Registry::new();
        assert!(matches!(r.register(Arc::new(BadManifest)), Err(RegistryError::InvalidManifest(_, _))));
    }

    struct NameMismatch;
    impl Organelle for NameMismatch {
        fn name(&self) -> &'static str {
            "psw-real"
        }
        fn manifest(&self) -> OrganelleManifest {
            sample_manifest("psw-other")
        }
        fn call(&self, _c: &str, _i: Value) -> Result<Value, OrganelleError> {
            Ok(Value::Null)
        }
    }

    #[test]
    fn name_mismatch_refused() {
        let r = Registry::new();
        assert!(r.register(Arc::new(NameMismatch)).is_err());
    }
}
