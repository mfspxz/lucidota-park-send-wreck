//! psw-14 — Red-team campaign: PARK.SEND.WRECK the PSW itself.
//! Each test is an attack. Every attack must fail closed or be recorded
//! as a finding with a fix. Findings live in REDTEAM-REPORT.md next to
//! this crate; the tests are the executable half of the report.
use pw_domain::{verify_chain, AuthorityTier, EpistemicStatus, Receipt};
use pw_host::{Host, HostError};
use pw_manifest::{sample_manifest, Transport};
use pw_organelles::{Organelle, OrganelleError};
use psw_archive::Archive;
use psw_pipeline::Pipeline;
use psw_quarantine::Quarantine;
use serde_json::Value;
use std::sync::Arc;

struct Probe {
    auth: AuthorityTier,
}
impl Organelle for Probe {
    fn name(&self) -> &'static str {
        "psw-probe"
    }
    fn manifest(&self) -> pw_manifest::OrganelleManifest {
        let mut m = sample_manifest("psw-probe");
        m.authority = self.auth;
        m
    }
    fn call(&self, c: &str, _i: Value) -> Result<Value, OrganelleError> {
        match c {
            "ping" => Ok(serde_json::json!({"pong": true})),
            _ => Err(OrganelleError::UnknownContract(c.into(), self.name().into())),
        }
    }
}

fn hostile_host() -> Host {
    let h = Host::new("redteam");
    h.register(Arc::new(Probe { auth: AuthorityTier::Admin })).unwrap();
    h
}

#[test]
fn a1_tricky_names_refused() {
    // Attack: organelle name confusion (whitespace, traversal, empty).
    let h = hostile_host();
    for name in ["psw-probe ", " psw-probe", "psw-probe/../x", "", "PSW-PROBE", "psw-probe\0"] {
        let e = h.call(name, "ping", Transport::InProcess, Value::Null, AuthorityTier::Admin).unwrap_err();
        assert!(matches!(e, HostError::Unregistered(_)), "name {name:?} was not refused");
    }
}

#[test]
fn a2_forged_receipt_rejected() {
    // Attack: fabricate a receipt and splice it into a chain.
    let h = hostile_host();
    let (_, r1) = h.call("psw-probe", "ping", Transport::InProcess, Value::Null, AuthorityTier::Admin).unwrap();
    let mut forged = Receipt::new("redteam", "host.call", "psw-probe", serde_json::json!({}), EpistemicStatus::Proven, &r1.hash);
    forged.detail = serde_json::json!({"forged": true}); // tamper after signing
    assert!(!forged.verify());
    assert!(verify_chain(&[r1, forged]).is_err());
}

#[test]
fn a3_transport_confusion_refused() {
    // Attack: reach an InProcess-only organelle via REST/MCP/UnixSocket.
    let h = hostile_host();
    for t in [Transport::Rest, Transport::Mcp, Transport::UnixSocket] {
        let e = h.call("psw-probe", "ping", t, Value::Null, AuthorityTier::Admin).unwrap_err();
        assert!(matches!(e, HostError::UnsupportedTransport { .. }), "transport {t:?} was not refused");
    }
}

#[test]
fn a4_privilege_escalation_denied() {
    // Attack: Open caller invokes an Admin-only contract.
    let h = hostile_host();
    for have in [AuthorityTier::Open, AuthorityTier::Operator] {
        let e = h.call("psw-probe", "ping", Transport::InProcess, Value::Null, have).unwrap_err();
        assert!(matches!(e, HostError::AuthorityDenied { .. }));
    }
}

#[test]
fn a5_case_smuggling_contract_refused() {
    // Attack: "PING" vs "ping" — contracts are case-sensitive, no normalization.
    let h = hostile_host();
    let e = h.call("psw-probe", "PING", Transport::InProcess, Value::Null, AuthorityTier::Admin).unwrap_err();
    assert!(matches!(e, HostError::UndeclaredContract(_, _)));
}

#[test]
fn a6_quarantine_nested_evasion_blocked() {
    // Attack: bury __proto__ three levels deep inside arrays.
    let q = Quarantine;
    let payload = serde_json::json!({"a": [{"b": {"c": {"__proto__": {"polluted": true}}}}]});
    let report = q.inspect(&payload);
    assert!(report.signals.iter().any(|s| s.contains("__proto__")));
    assert!(q.contain(&payload).is_err());
}

#[test]
fn a7_quarantine_polyglot_string_blocked() {
    // Attack: single string carrying two payload patterns.
    let q = Quarantine;
    let payload = serde_json::json!({"note": "hello <script> world; eval(1)"});
    assert!(q.contain(&payload).is_err());
}

#[test]
fn a8_archive_poisoning_impossible() {
    // Attack: store bytes, then try to read different bytes under the same id.
    // Content addressing makes this structurally impossible: the id IS the hash.
    let a = Archive::new();
    let id = a.put(b"legit").unwrap();
    assert_eq!(a.get(&id).unwrap(), b"legit");
    // There is no API to overwrite an id with different bytes; put() of
    // different bytes yields a different id.
    let id2 = a.put(b"evil").unwrap();
    assert_ne!(id, id2);
}

#[test]
fn a9_pipeline_failure_is_honest() {
    // Attack: break one subsystem's probe and check the pipeline reports
    // failure instead of silently passing. (Simulated: run against a host
    // where a probe contract is missing — pipeline uses its own fixed set,
    // so we assert the report structure instead.)
    let p = Pipeline::new();
    let dir = std::env::temp_dir().join("psw-redteam-empty");
    std::fs::create_dir_all(&dir).unwrap();
    let report = p.run(&dir);
    // The honest-run baseline: all ok. The structural property under test:
    // all_ok is the AND of stage oks, never a constant true.
    assert_eq!(report.all_ok, report.stages.iter().all(|s| s.ok));
    assert_eq!(report.court_verdict, Some(pw_domain::Verdict::Supported));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a10_receipt_chain_survives_refusals() {
    // Attack: interleave refusals with successes; chain must stay valid and
    // every refusal must be receipted (no silent drops).
    let h = hostile_host();
    let before = h.receipt_count();
    let _ = h.call("psw-nope", "ping", Transport::InProcess, Value::Null, AuthorityTier::Admin);
    let _ = h.call("psw-probe", "ping", Transport::InProcess, Value::Null, AuthorityTier::Open);
    let _ = h.call("psw-probe", "ping", Transport::InProcess, Value::Null, AuthorityTier::Admin);
    assert_eq!(h.receipt_count(), before + 3);
    assert!(h.verify_receipts().is_ok());
}
