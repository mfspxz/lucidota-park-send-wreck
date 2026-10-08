//! org-002 — parkwreck-organelle-host rebuild (proven artifact name).
//! The host: loads manifests, routes calls, enforces fail-closed.
//! Rules:
//!   1. Unregistered organelle  -> refused, nothing executes.
//!   2. Undeclared contract    -> refused, nothing executes.
//!   3. Unsupported transport  -> refused.
//!   4. Insufficient authority  -> refused.
//!   5. Every call emits a chained receipt (per organelle receipt policy).
use pw_domain::{verify_chain, AuthorityTier, DomainError, EpistemicStatus, Receipt};
use pw_manifest::{OrganelleManifest, Transport};
use pw_organelles::{Organelle, OrganelleError, Registry};
use serde_json::Value;
use std::sync::{Arc, Mutex};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum HostError {
    #[error("organelle not registered: {0:?}")]
    Unregistered(String),
    #[error("contract not declared: organelle={0:?} contract={1:?}")]
    UndeclaredContract(String, String),
    #[error("transport {transport:?} not supported by {organelle:?}")]
    UnsupportedTransport { organelle: String, transport: Transport },
    #[error("authority denied: need {need:?}, have {have:?}")]
    AuthorityDenied { need: AuthorityTier, have: AuthorityTier },
    #[error("manifest: {0}")]
    Manifest(String),
    #[error("organelle: {0}")]
    Organelle(#[from] OrganelleError),
    #[error("domain: {0}")]
    Domain(#[from] DomainError),
}

pub struct CallRecord {
    pub organelle: String,
    pub contract: String,
    pub transport: Transport,
    pub receipt: Receipt,
    pub ok: bool,
}

/// The host. Owns the registry and the receipt chain.
pub struct Host {
    registry: Registry,
    receipts: Mutex<Vec<Receipt>>,
    actor: String,
}

impl Host {
    pub fn new(actor: impl Into<String>) -> Self {
        Host { registry: Registry::new(), receipts: Mutex::new(Vec::new()), actor: actor.into() }
    }

    /// Load + validate manifests from a directory of JSON files.
    /// Fail-closed: any invalid manifest aborts the whole load.
    pub fn load_manifests_dir(&self, dir: &std::path::Path) -> Result<Vec<String>, HostError> {
        let mut names = Vec::new();
        let mut files: Vec<_> = std::fs::read_dir(dir)
            .map_err(|e| HostError::Manifest(e.to_string()))?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().map(|x| x == "json").unwrap_or(false))
            .collect();
        files.sort();
        // Pre-validate ALL before registering ANY (atomic load).
        let mut parsed: Vec<OrganelleManifest> = Vec::new();
        for f in &files {
            let s = std::fs::read_to_string(f).map_err(|e| HostError::Manifest(e.to_string()))?;
            let m = OrganelleManifest::from_json(&s).map_err(|e| HostError::Manifest(e.to_string()))?;
            m.validate().map_err(|e| HostError::Manifest(format!("{}: {e}", f.display())))?;
            parsed.push(m);
        }
        for m in parsed {
            names.push(m.name.clone());
            // Registration needs the impl; dir-load only records manifests.
            // Real impls register via `register()`.
        }
        Ok(names)
    }

    /// Register a live organelle implementation. Manifest re-validated.
    pub fn register(&self, o: Arc<dyn Organelle>) -> Result<(), HostError> {
        self.registry
            .register(o)
            .map_err(|e| HostError::Manifest(e.to_string()))
    }

    fn emit(&self, action: &str, subject: &str, detail: Value, ok: bool) -> Receipt {
        let mut chain = self.receipts.lock().unwrap();
        let prev = chain.last().map(|r| r.hash.clone()).unwrap_or_else(|| "GENESIS".into());
        let ep = if ok { EpistemicStatus::Proven } else { EpistemicStatus::Disproven };
        let r = if chain.is_empty() {
            let mut g = Receipt::genesis(&self.actor, action, subject);
            g.detail = detail;
            g.epistemic = ep;
            // recompute hash with real detail
            Receipt::new(&self.actor, action, subject, g.detail.clone(), ep, "GENESIS")
        } else {
            Receipt::new(&self.actor, action, subject, detail, ep, &prev)
        };
        chain.push(r.clone());
        r
    }

    /// Route one call. Every refusal path emits a receipt and executes nothing.
    pub fn call(
        &self,
        organelle: &str,
        contract: &str,
        transport: Transport,
        input: Value,
        authority: AuthorityTier,
    ) -> Result<(Value, Receipt), HostError> {
        let fail = |e: HostError| -> Result<(Value, Receipt), HostError> {
            let r = self.emit(
                "host.call.refused",
                organelle,
                serde_json::json!({"contract": contract, "transport": format!("{transport:?}"), "error": e.to_string()}),
                false,
            );
            let _ = r;
            Err(e)
        };
        let o = match self.registry.get(organelle) {
            Ok(o) => o,
            Err(_) => return fail(HostError::Unregistered(organelle.into())),
        };
        let m = o.manifest();
        if !m.has_contract(contract) {
            return fail(HostError::UndeclaredContract(organelle.into(), contract.into()));
        }
        if !m.supports(transport) {
            return fail(HostError::UnsupportedTransport { organelle: organelle.into(), transport });
        }
        if !authority.satisfies(m.authority) {
            return fail(HostError::AuthorityDenied { need: m.authority, have: authority });
        }
        match o.call(contract, input.clone()) {
            Ok(out) => {
                let r = self.emit(
                    "host.call",
                    organelle,
                    serde_json::json!({"contract": contract, "transport": format!("{transport:?}"),
                                       "out_digest": pw_domain::hash_hex(&out.to_string())}),
                    true,
                );
                Ok((out, r))
            }
            Err(e) => {
                let he = HostError::Organelle(e);
                let r = self.emit(
                    "host.call.error",
                    organelle,
                    serde_json::json!({"contract": contract, "error": he.to_string()}),
                    false,
                );
                let _ = r;
                Err(he)
            }
        }
    }

    pub fn organelles(&self) -> Vec<String> {
        self.registry.names()
    }

    /// Direct registry access (used by the pipeline's inspect stage).
    pub fn get_organelle(&self, name: &str) -> Option<Arc<dyn Organelle>> {
        self.registry.get(name).ok()
    }

    pub fn verify_receipts(&self) -> Result<(), DomainError> {
        verify_chain(&self.receipts.lock().unwrap())
    }

    pub fn receipt_count(&self) -> usize {
        self.receipts.lock().unwrap().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pw_manifest::{sample_manifest, Contract, ReceiptPolicy};
    use pw_organelles::OrganelleError;
    use std::sync::Arc;

    struct Ping {
        auth: AuthorityTier,
        transports: Vec<Transport>,
    }
    impl Organelle for Ping {
        fn name(&self) -> &'static str {
            "psw-ping"
        }
        fn manifest(&self) -> OrganelleManifest {
            let mut m = sample_manifest("psw-ping");
            m.authority = self.auth;
            m.transports = self.transports.clone();
            m.contracts = vec![
                Contract { name: "ping".into(), input_schema: "{}".into(), output_schema: "{}".into(), idempotent: true },
                Contract { name: "fail".into(), input_schema: "{}".into(), output_schema: "{}".into(), idempotent: false },
            ];
            m.receipts = ReceiptPolicy::Always;
            m
        }
        fn call(&self, contract: &str, _input: Value) -> Result<Value, OrganelleError> {
            match contract {
                "ping" => Ok(serde_json::json!({"pong": true})),
                "fail" => Err(OrganelleError::Execution("boom".into())),
                _ => Err(OrganelleError::UnknownContract(contract.into(), self.name().into())),
            }
        }
    }

    fn host() -> Host {
        let h = Host::new("test-host");
        h.register(Arc::new(Ping { auth: AuthorityTier::Operator, transports: vec![Transport::InProcess] })).unwrap();
        h
    }

    #[test] fn t01_route_ok() {
        let h = host();
        let (out, r) = h.call("psw-ping", "ping", Transport::InProcess, Value::Null, AuthorityTier::Operator).unwrap();
        assert_eq!(out["pong"], true);
        assert!(r.verify());
    }
    #[test] fn t02_unknown_organelle_refused() {
        let h = host();
        let e = h.call("psw-nope", "ping", Transport::InProcess, Value::Null, AuthorityTier::Admin).unwrap_err();
        assert!(matches!(e, HostError::Unregistered(_)));
    }
    #[test] fn t03_undeclared_contract_refused() {
        let h = host();
        let e = h.call("psw-ping", "nope", Transport::InProcess, Value::Null, AuthorityTier::Admin).unwrap_err();
        assert!(matches!(e, HostError::UndeclaredContract(_, _)));
    }
    #[test] fn t04_unsupported_transport_refused() {
        let h = host();
        let e = h.call("psw-ping", "ping", Transport::Rest, Value::Null, AuthorityTier::Admin).unwrap_err();
        assert!(matches!(e, HostError::UnsupportedTransport { .. }));
    }
    #[test] fn t05_authority_denied_fail_closed() {
        let h = host();
        let e = h.call("psw-ping", "ping", Transport::InProcess, Value::Null, AuthorityTier::Open).unwrap_err();
        assert!(matches!(e, HostError::AuthorityDenied { .. }));
    }
    #[test] fn t06_higher_authority_ok() {
        let h = host();
        assert!(h.call("psw-ping", "ping", Transport::InProcess, Value::Null, AuthorityTier::Admin).is_ok());
    }
    #[test] fn t07_refusal_emits_receipt() {
        let h = host();
        let before = h.receipt_count();
        let _ = h.call("psw-nope", "ping", Transport::InProcess, Value::Null, AuthorityTier::Admin);
        assert_eq!(h.receipt_count(), before + 1);
    }
    #[test] fn t08_organelle_error_propagates_with_receipt() {
        let h = host();
        let before = h.receipt_count();
        let e = h.call("psw-ping", "fail", Transport::InProcess, Value::Null, AuthorityTier::Operator).unwrap_err();
        assert!(matches!(e, HostError::Organelle(_)));
        assert_eq!(h.receipt_count(), before + 1);
    }
    #[test] fn t09_receipt_chain_verifies() {
        let h = host();
        let _ = h.call("psw-ping", "ping", Transport::InProcess, Value::Null, AuthorityTier::Operator);
        let _ = h.call("psw-nope", "ping", Transport::InProcess, Value::Null, AuthorityTier::Operator);
        let _ = h.call("psw-ping", "ping", Transport::InProcess, Value::Null, AuthorityTier::Operator);
        assert!(h.verify_receipts().is_ok());
    }
    #[test] fn t10_refused_call_executes_nothing() {
        // Ping has no side effects to observe; assert the error type proves
        // dispatch never reached the organelle (UnknownContract would differ).
        let h = host();
        let e = h.call("psw-ping", "undeclared", Transport::InProcess, Value::Null, AuthorityTier::Admin).unwrap_err();
        assert!(matches!(e, HostError::UndeclaredContract(_, _)));
    }
    #[test] fn t11_manifest_dir_load_valid() {
        let dir = std::env::temp_dir().join("psw-host-test-valid");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.json"), serde_json::to_string(&sample_manifest("psw-a")).unwrap()).unwrap();
        let h = Host::new("t");
        let names = h.load_manifests_dir(&dir).unwrap();
        assert_eq!(names, vec!["psw-a"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }
    #[test] fn t12_manifest_dir_load_invalid_aborts() {
        let dir = std::env::temp_dir().join("psw-host-test-invalid");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.json"), serde_json::to_string(&sample_manifest("psw-a")).unwrap()).unwrap();
        let mut bad = sample_manifest("psw-b");
        bad.contracts.clear();
        std::fs::write(dir.join("b.json"), serde_json::to_string(&bad).unwrap()).unwrap();
        let h = Host::new("t");
        assert!(h.load_manifests_dir(&dir).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }
    #[test] fn t13_manifest_dir_missing_is_error() {
        let h = Host::new("t");
        assert!(h.load_manifests_dir(std::path::Path::new("/nonexistent-psw-dir")).is_err());
    }
    #[test] fn t14_register_invalid_manifest_refused() {
        struct Bad;
        impl Organelle for Bad {
            fn name(&self) -> &'static str { "psw-bad2" }
            fn manifest(&self) -> OrganelleManifest { let mut m = sample_manifest("psw-bad2"); m.transports.clear(); m }
            fn call(&self, _c: &str, _i: Value) -> Result<Value, OrganelleError> { Ok(Value::Null) }
        }
        let h = Host::new("t");
        assert!(h.register(Arc::new(Bad)).is_err());
    }
    #[test] fn t15_duplicate_register_refused() {
        let h = Host::new("t");
        let p = || Arc::new(Ping { auth: AuthorityTier::Open, transports: vec![Transport::InProcess] });
        h.register(p()).unwrap();
        assert!(h.register(p()).is_err());
    }
    #[test] fn t16_organelles_listed() {
        let h = host();
        assert_eq!(h.organelles(), vec!["psw-ping"]);
    }
    #[test] fn t17_open_tier_callable_by_all() {
        let h = Host::new("t");
        h.register(Arc::new(Ping { auth: AuthorityTier::Open, transports: vec![Transport::InProcess] })).unwrap();
        assert!(h.call("psw-ping", "ping", Transport::InProcess, Value::Null, AuthorityTier::Open).is_ok());
    }
    #[test] fn t18_empty_chain_verifies() {
        let h = Host::new("t");
        assert!(h.verify_receipts().is_ok());
    }
    #[test] fn t19_receipt_links_to_previous() {
        let h = host();
        let (_, r1) = h.call("psw-ping", "ping", Transport::InProcess, Value::Null, AuthorityTier::Operator).unwrap();
        let (_, r2) = h.call("psw-ping", "ping", Transport::InProcess, Value::Null, AuthorityTier::Operator).unwrap();
        assert_eq!(r2.prev_hash, r1.hash);
    }
    #[test] fn t20_output_digest_recorded() {
        let h = host();
        let (out, r) = h.call("psw-ping", "ping", Transport::InProcess, Value::Null, AuthorityTier::Operator).unwrap();
        let digest = pw_domain::hash_hex(&out.to_string());
        assert_eq!(r.detail["out_digest"].as_str().unwrap(), digest);
    }
}
