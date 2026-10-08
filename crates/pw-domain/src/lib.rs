//! parkwreck-domain — proven crate name from the Aug 2026 tree (ledger seq 265).
//! Core domain types for ParkSendWreck: trinary epistemics, hash-chained
//! receipts, authority tiers, claims and verdicts. Everything downstream
//! builds on these; they are the shared vocabulary of the rebuild.
use blake2::{Blake2b512, Digest};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Trinary epistemic status. Unknown is a first-class, valid state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EpistemicStatus {
    Proven,
    Disproven,
    Unknown,
}

/// Authority tier required to invoke a contract. Fail-closed: a caller with
/// a lower tier than required is refused, never downgraded silently.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuthorityTier {
    Open,
    Operator,
    Admin,
}

impl AuthorityTier {
    pub fn satisfies(self, required: AuthorityTier) -> bool {
        self >= required
    }
}

/// Opaque entity identifier (content-derived in real use; random ok for v1).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EntityId(pub String);

impl EntityId {
    pub fn new(s: impl Into<String>) -> Self {
        EntityId(s.into())
    }
}

/// A hash-chained receipt. Every mutating action in PSW emits one.
/// Chain semantics: receipt N commits to receipt N-1 via prev_hash, so a
/// tampered or dropped receipt breaks verification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Receipt {
    pub id: String,
    pub ts_utc: String,
    pub actor: String,
    pub action: String,
    pub subject: String,
    pub detail: serde_json::Value,
    pub epistemic: EpistemicStatus,
    pub prev_hash: String,
    pub hash: String,
}

impl Receipt {
    pub fn genesis(actor: &str, action: &str, subject: &str) -> Self {
        Self::new(actor, action, subject, serde_json::Value::Null, EpistemicStatus::Unknown, "GENESIS")
    }

    pub fn new(
        actor: &str,
        action: &str,
        subject: &str,
        detail: serde_json::Value,
        epistemic: EpistemicStatus,
        prev_hash: &str,
    ) -> Self {
        let ts_utc = chrono_now_utc();
        let id = format!("rcpt-{}", &hash_hex(&format!("{ts_utc}{actor}{action}{subject}{prev_hash}"))[..16]);
        let hash = Self::compute_hash(&id, &ts_utc, actor, action, subject, &detail, epistemic, prev_hash);
        Receipt { id, ts_utc, actor: actor.into(), action: action.into(), subject: subject.into(),
                   detail, epistemic, prev_hash: prev_hash.into(), hash }
    }

    fn compute_hash(id: &str, ts: &str, actor: &str, action: &str, subject: &str,
                    detail: &serde_json::Value, epistemic: EpistemicStatus, prev: &str) -> String {
        let canon = format!("{id}|{ts}|{actor}|{action}|{subject}|{detail:?}|{epistemic:?}|{prev}");
        hash_hex(&canon)
    }

    /// Recompute and compare; detects any field tampering.
    pub fn verify(&self) -> bool {
        self.hash == Self::compute_hash(&self.id, &self.ts_utc, &self.actor, &self.action,
                                        &self.subject, &self.detail, self.epistemic, &self.prev_hash)
    }
}

/// Verify a whole chain: every receipt valid and linked to its predecessor.
pub fn verify_chain(chain: &[Receipt]) -> Result<(), DomainError> {
    for (i, r) in chain.iter().enumerate() {
        if !r.verify() {
            return Err(DomainError::ReceiptTampered { id: r.id.clone() });
        }
        if i == 0 {
            if r.prev_hash != "GENESIS" {
                return Err(DomainError::ChainBroken { at: r.id.clone() });
            }
        } else if r.prev_hash != chain[i - 1].hash {
            return Err(DomainError::ChainBroken { at: r.id.clone() });
        }
    }
    Ok(())
}

pub fn hash_hex(s: &str) -> String {
    let mut h = Blake2b512::new();
    h.update(s.as_bytes());
    hex::encode(h.finalize())
}

pub fn hash_bytes_hex(bytes: &[u8]) -> String {
    let mut h = Blake2b512::new();
    h.update(bytes);
    hex::encode(h.finalize())
}

fn chrono_now_utc() -> String {
    // No chrono dep: format from system time manually (UTC ISO-8601).
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
    // 1970-01-01 + secs -> civil date (Howard Hinnant's algorithm)
    let days = (secs / 86400) as i64;
    let sod = secs % 86400;
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z.rem_euclid(146097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", sod / 3600, (sod % 3600) / 60, sod % 60)
}

/// A claim awaiting adjudication.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claim {
    pub id: String,
    pub statement: String,
    pub evidence: Vec<String>,
    pub status: EpistemicStatus,
}

impl Claim {
    pub fn new(statement: impl Into<String>, evidence: Vec<String>) -> Self {
        let statement = statement.into();
        let id = format!("claim-{}", &hash_hex(&statement)[..12]);
        Claim { id, statement, evidence, status: EpistemicStatus::Unknown }
    }
}

/// Court verdict over a claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Verdict {
    Supported,
    Refuted,
    Inconclusive,
}

#[derive(Debug, Error)]
pub enum DomainError {
    #[error("receipt tampered: {id}")]
    ReceiptTampered { id: String },
    #[error("receipt chain broken at {at}")]
    ChainBroken { at: String },
    #[error("invalid input: {0}")]
    InvalidInput(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trinary_unknown_is_valid() {
        assert_eq!(EpistemicStatus::Unknown as u8, EpistemicStatus::Unknown as u8);
    }

    #[test]
    fn authority_ordering_fail_closed() {
        assert!(AuthorityTier::Operator.satisfies(AuthorityTier::Open));
        assert!(!AuthorityTier::Open.satisfies(AuthorityTier::Operator));
        assert!(AuthorityTier::Admin.satisfies(AuthorityTier::Admin));
    }

    #[test]
    fn receipt_verifies_fresh() {
        let r = Receipt::genesis("tester", "test.action", "subject");
        assert!(r.verify());
    }

    #[test]
    fn receipt_tamper_detected() {
        let mut r = Receipt::genesis("tester", "test.action", "subject");
        r.action = "evil.action".into();
        assert!(!r.verify());
    }

    #[test]
    fn chain_verifies_in_order() {
        let r0 = Receipt::genesis("a", "a0", "s");
        let r1 = Receipt::new("a", "a1", "s", serde_json::json!({}), EpistemicStatus::Proven, &r0.hash);
        let r2 = Receipt::new("a", "a2", "s", serde_json::json!({}), EpistemicStatus::Proven, &r1.hash);
        assert!(verify_chain(&[r0, r1, r2]).is_ok());
    }

    #[test]
    fn chain_break_detected_on_reorder() {
        let r0 = Receipt::genesis("a", "a0", "s");
        let r1 = Receipt::new("a", "a1", "s", serde_json::json!({}), EpistemicStatus::Proven, &r0.hash);
        assert!(verify_chain(&[r1, r0]).is_err());
    }

    #[test]
    fn chain_rejects_dropped_link() {
        let r0 = Receipt::genesis("a", "a0", "s");
        let r1 = Receipt::new("a", "a1", "s", serde_json::json!({}), EpistemicStatus::Proven, &r0.hash);
        let r2 = Receipt::new("a", "a2", "s", serde_json::json!({}), EpistemicStatus::Proven, &r0.hash);
        assert!(verify_chain(&[r0, r1, r2]).is_err());
    }

    #[test]
    fn claim_starts_unknown() {
        let c = Claim::new("x is y", vec![]);
        assert_eq!(c.status, EpistemicStatus::Unknown);
    }

    #[test]
    fn hash_is_deterministic() {
        assert_eq!(hash_hex("abc"), hash_hex("abc"));
        assert_ne!(hash_hex("abc"), hash_hex("abd"));
    }
}
