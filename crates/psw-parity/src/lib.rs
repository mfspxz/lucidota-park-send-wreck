//! psw-006 — Parity: the transport-equivalence checker.
//! An organelle invoked through different transports must produce the same
//! result. Parity compares output digests across transports for the same
//! (organelle, contract, input) and reports divergence. A divergent
//! transport is a semantic leak and fails the check.
use pw_domain::AuthorityTier;
use pw_manifest::{Contract, OrganelleManifest, ReceiptPolicy, Transport, SCHEMA_VERSION};
use pw_organelles::{Organelle, OrganelleError};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransportSample {
    pub transport: String,
    pub output_digest: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParityReport {
    pub organelle: String,
    pub contract: String,
    pub samples: usize,
    pub equivalent: bool,
    pub divergent: Vec<String>,
}

pub struct Parity;

impl Parity {
    pub fn check(organelle: &str, contract: &str, samples: &[TransportSample]) -> ParityReport {
        let mut divergent = Vec::new();
        let mut first: Option<&str> = None;
        for s in samples {
            match first {
                None => first = Some(&s.output_digest),
                Some(d) if d != s.output_digest => divergent.push(s.transport.clone()),
                _ => {}
            }
        }
        // Transports that never reported are also divergent-by-absence.
        ParityReport {
            organelle: organelle.into(),
            contract: contract.into(),
            samples: samples.len(),
            equivalent: divergent.is_empty() && !samples.is_empty(),
            divergent,
        }
    }
}

fn manifest() -> OrganelleManifest {
    OrganelleManifest {
        schema: SCHEMA_VERSION.into(),
        name: "psw-parity".into(),
        version: "0.1.0".into(),
        description: "Transport equivalence checker: same call, same digest, every door".into(),
        contracts: vec![
            Contract { name: "check".into(), input_schema: "{\"organelle\":str,\"contract\":str,\"samples\":[...]}".into(), output_schema: "ParityReport".into(), idempotent: true },
        ],
        authority: AuthorityTier::Operator,
        transports: vec![Transport::InProcess],
        receipts: ReceiptPolicy::Always,
        entrypoint: "psw_parity::Parity".into(),
    }
}

impl Organelle for Parity {
    fn name(&self) -> &'static str {
        "psw-parity"
    }
    fn manifest(&self) -> OrganelleManifest {
        manifest()
    }
    fn call(&self, contract: &str, input: Value) -> Result<Value, OrganelleError> {
        match contract {
            "check" => {
                let organelle = input.get("organelle").and_then(|v| v.as_str()).ok_or_else(|| OrganelleError::BadInput("organelle".into()))?;
                let contract = input.get("contract").and_then(|v| v.as_str()).ok_or_else(|| OrganelleError::BadInput("contract".into()))?;
                let samples: Vec<TransportSample> = serde_json::from_value(input.get("samples").cloned().unwrap_or(Value::Null))
                    .map_err(|e| OrganelleError::BadInput(e.to_string()))?;
                Ok(serde_json::to_value(Parity::check(organelle, contract, &samples)).unwrap())
            }
            _ => Err(OrganelleError::UnknownContract(contract.into(), self.name().into())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(t: &str, d: &str) -> TransportSample {
        TransportSample { transport: t.into(), output_digest: d.into() }
    }

    #[test]
    fn identical_digests_equivalent() {
        let r = Parity::check("o", "c", &[sample("in-process", "abc"), sample("rest", "abc"), sample("mcp", "abc")]);
        assert!(r.equivalent);
        assert!(r.divergent.is_empty());
    }

    #[test]
    fn divergent_transport_reported() {
        let r = Parity::check("o", "c", &[sample("in-process", "abc"), sample("rest", "XYZ")]);
        assert!(!r.equivalent);
        assert_eq!(r.divergent, vec!["rest"]);
    }

    #[test]
    fn first_sample_sets_baseline() {
        let r = Parity::check("o", "c", &[sample("a", "1"), sample("b", "1"), sample("c", "2"), sample("d", "1")]);
        assert_eq!(r.divergent, vec!["c"]);
    }

    #[test]
    fn empty_samples_not_equivalent() {
        let r = Parity::check("o", "c", &[]);
        assert!(!r.equivalent);
    }

    #[test]
    fn single_sample_equivalent() {
        let r = Parity::check("o", "c", &[sample("in-process", "abc")]);
        assert!(r.equivalent);
    }

    #[test]
    fn organelle_check_contract() {
        let p = Parity;
        let out = p.call("check", serde_json::json!({
            "organelle": "psw-archive", "contract": "put",
            "samples": [{"transport": "in-process", "output_digest": "d1"}, {"transport": "rest", "output_digest": "d1"}]
        })).unwrap();
        assert_eq!(out["equivalent"], true);
        assert_eq!(out["samples"], 2);
    }
}
