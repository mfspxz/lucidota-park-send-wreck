//! org-001 — Organelle manifest schema v1.
//! A manifest is the single source of truth for what an organelle claims to
//! be: name, version, contracts, authority, transports, receipts.
//! The host trusts NOTHING else about an organelle. Validation is
//! fail-closed: any schema violation refuses registration.
use pw_domain::AuthorityTier;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const SCHEMA_VERSION: &str = "psw-organelle-manifest/v1";

/// A named operation an organelle exposes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contract {
    pub name: String,
    pub input_schema: String,
    pub output_schema: String,
    #[serde(default)]
    pub idempotent: bool,
}

/// Transport surface. Behavioral equivalence across transports is enforced
/// by the Parity subsystem; the manifest only declares support.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Transport {
    InProcess,
    UnixSocket,
    Rest,
    Mcp,
}

/// When the host must emit call receipts for this organelle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReceiptPolicy {
    Always,
    OnError,
    Never,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganelleManifest {
    pub schema: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub contracts: Vec<Contract>,
    pub authority: AuthorityTier,
    pub transports: Vec<Transport>,
    pub receipts: ReceiptPolicy,
    pub entrypoint: String,
}

#[derive(Debug, Error)]
pub enum ManifestError {
    #[error("schema mismatch: expected {SCHEMA_VERSION}, got {0}")]
    SchemaMismatch(String),
    #[error("invalid name {0:?}: must be kebab-case [a-z0-9-], 3..=64 chars")]
    BadName(String),
    #[error("invalid version {0:?}: must be semver x.y.z")]
    BadVersion(String),
    #[error("organelle must declare at least one contract")]
    NoContracts,
    #[error("duplicate contract name {0:?}")]
    DuplicateContract(String),
    #[error("invalid contract name {0:?}")]
    BadContractName(String),
    #[error("organelle must declare at least one transport")]
    NoTransports,
    #[error("empty entrypoint")]
    EmptyEntrypoint,
    #[error("json error: {0}")]
    Json(String),
}

fn is_kebab(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !s.starts_with('-')
        && !s.ends_with('-')
}

fn is_semver(s: &str) -> bool {
    let parts: Vec<&str> = s.split('.').collect();
    parts.len() == 3 && parts.iter().all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
}

impl OrganelleManifest {
    /// Fail-closed validation. Returns Ok(()) only if every rule holds.
    pub fn validate(&self) -> Result<(), ManifestError> {
        if self.schema != SCHEMA_VERSION {
            return Err(ManifestError::SchemaMismatch(self.schema.clone()));
        }
        if !is_kebab(&self.name) || self.name.len() < 3 {
            return Err(ManifestError::BadName(self.name.clone()));
        }
        if !is_semver(&self.version) {
            return Err(ManifestError::BadVersion(self.version.clone()));
        }
        if self.contracts.is_empty() {
            return Err(ManifestError::NoContracts);
        }
        let mut seen = std::collections::HashSet::new();
        for c in &self.contracts {
            if !is_kebab(&c.name) {
                return Err(ManifestError::BadContractName(c.name.clone()));
            }
            if !seen.insert(c.name.clone()) {
                return Err(ManifestError::DuplicateContract(c.name.clone()));
            }
        }
        if self.transports.is_empty() {
            return Err(ManifestError::NoTransports);
        }
        if self.entrypoint.trim().is_empty() {
            return Err(ManifestError::EmptyEntrypoint);
        }
        Ok(())
    }

    pub fn from_json(s: &str) -> Result<Self, ManifestError> {
        serde_json::from_str(s).map_err(|e| ManifestError::Json(e.to_string()))
    }

    pub fn has_contract(&self, name: &str) -> bool {
        self.contracts.iter().any(|c| c.name == name)
    }

    pub fn supports(&self, t: Transport) -> bool {
        self.transports.contains(&t)
    }
}

/// Canonical valid manifest used across host tests.
pub fn sample_manifest(name: &str) -> OrganelleManifest {
    OrganelleManifest {
        schema: SCHEMA_VERSION.into(),
        name: name.into(),
        version: "0.1.0".into(),
        description: "sample".into(),
        contracts: vec![Contract {
            name: "ping".into(),
            input_schema: "{}".into(),
            output_schema: "{\"pong\":bool}".into(),
            idempotent: true,
        }],
        authority: AuthorityTier::Open,
        transports: vec![Transport::InProcess],
        receipts: ReceiptPolicy::Always,
        entrypoint: format!("{name}::organelle"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_validates() {
        sample_manifest("psw-sample").validate().unwrap();
    }

    #[test]
    fn rejects_wrong_schema() {
        let mut m = sample_manifest("psw-sample");
        m.schema = "other/v9".into();
        assert!(matches!(m.validate(), Err(ManifestError::SchemaMismatch(_))));
    }

    #[test]
    fn rejects_bad_names() {
        for bad in ["ab", "HasCaps", "has space", "-lead", "trail-", "has_under", ""] {
            let mut m = sample_manifest("psw-sample");
            m.name = bad.into();
            assert!(m.validate().is_err(), "name {bad:?} should fail");
        }
    }

    #[test]
    fn rejects_bad_versions() {
        for bad in ["1", "1.2", "v1.2.3", "1.2.3.4", "a.b.c", ""] {
            let mut m = sample_manifest("psw-sample");
            m.version = bad.into();
            assert!(m.validate().is_err(), "version {bad:?} should fail");
        }
    }

    #[test]
    fn rejects_no_contracts() {
        let mut m = sample_manifest("psw-sample");
        m.contracts.clear();
        assert!(matches!(m.validate(), Err(ManifestError::NoContracts)));
    }

    #[test]
    fn rejects_duplicate_contracts() {
        let mut m = sample_manifest("psw-sample");
        m.contracts.push(m.contracts[0].clone());
        assert!(matches!(m.validate(), Err(ManifestError::DuplicateContract(_))));
    }

    #[test]
    fn rejects_no_transports() {
        let mut m = sample_manifest("psw-sample");
        m.transports.clear();
        assert!(matches!(m.validate(), Err(ManifestError::NoTransports)));
    }

    #[test]
    fn rejects_empty_entrypoint() {
        let mut m = sample_manifest("psw-sample");
        m.entrypoint = "   ".into();
        assert!(matches!(m.validate(), Err(ManifestError::EmptyEntrypoint)));
    }

    #[test]
    fn json_roundtrip() {
        let m = sample_manifest("psw-rt");
        let s = serde_json::to_string(&m).unwrap();
        let back = OrganelleManifest::from_json(&s).unwrap();
        back.validate().unwrap();
        assert_eq!(back.name, "psw-rt");
    }

    #[test]
    fn contract_lookup() {
        let m = sample_manifest("psw-lookup");
        assert!(m.has_contract("ping"));
        assert!(!m.has_contract("pong"));
        assert!(m.supports(Transport::InProcess));
        assert!(!m.supports(Transport::Rest));
    }
}
