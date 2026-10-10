//! psw-005 — Archive: the content-addressed vault.
//! Put bytes -> blake2-512 content id. Get id -> bytes or NotFound.
//! Content addressing means the id IS the integrity check: bytes that do
//! not hash to the requested id can never be returned for it.
mod organelle;
pub use organelle::*;
use pw_domain::{hash_bytes_hex, AuthorityTier};
use pw_manifest::{Contract, OrganelleManifest, ReceiptPolicy, Transport, SCHEMA_VERSION};
use pw_organelles::{Organelle, OrganelleError};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Debug, thiserror::Error)]
pub enum ArchiveError {
    #[error("not found: {0}")]
    NotFound(String),
    #[error("empty payload refused")]
    EmptyPayload,
}

pub struct Archive {
    store: Mutex<HashMap<String, Vec<u8>>>,
}

impl Archive {
    pub fn new() -> Self {
        Archive { store: Mutex::new(HashMap::new()) }
    }

    pub fn put(&self, bytes: &[u8]) -> Result<String, ArchiveError> {
        if bytes.is_empty() {
            return Err(ArchiveError::EmptyPayload);
        }
        let id = hash_bytes_hex(bytes);
        self.store.lock().unwrap().insert(id.clone(), bytes.to_vec());
        Ok(id)
    }

    pub fn get(&self, id: &str) -> Result<Vec<u8>, ArchiveError> {
        let store = self.store.lock().unwrap();
        let bytes = store.get(id).ok_or_else(|| ArchiveError::NotFound(id.into()))?;
        // Integrity re-check on read: the vault never trusts its own shelves.
        debug_assert_eq!(hash_bytes_hex(bytes), id);
        if hash_bytes_hex(bytes) != id {
            return Err(ArchiveError::NotFound(format!("integrity failure for {id}")));
        }
        Ok(bytes.clone())
    }

    pub fn contains(&self, id: &str) -> bool {
        self.store.lock().unwrap().contains_key(id)
    }

    pub fn len(&self) -> usize {
        self.store.lock().unwrap().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Default for Archive {
    fn default() -> Self {
        Archive::new()
    }
}

fn manifest() -> OrganelleManifest {
    OrganelleManifest {
        schema: SCHEMA_VERSION.into(),
        name: "psw-archive".into(),
        version: "0.1.0".into(),
        description: "Content-addressed vault: blake2 ids, integrity-checked reads".into(),
        contracts: vec![
            Contract { name: "put".into(), input_schema: "{\"bytes_b64\":str}".into(), output_schema: "{\"id\":str}".into(), idempotent: true },
            Contract { name: "get".into(), input_schema: "{\"id\":str}".into(), output_schema: "{\"bytes_b64\":str}".into(), idempotent: true },
            Contract { name: "contains".into(), input_schema: "{\"id\":str}".into(), output_schema: "{\"found\":bool}".into(), idempotent: true },
        ],
        authority: AuthorityTier::Operator,
        transports: vec![Transport::InProcess],
        receipts: ReceiptPolicy::Always,
        entrypoint: "psw_archive::Archive".into(),
    }
}

fn b64_encode(bytes: &[u8]) -> String {
    const ALPH: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::new();
    for chunk in bytes.chunks(3) {
        let mut n: u32 = 0;
        for (i, &b) in chunk.iter().enumerate() {
            n |= (b as u32) << (16 - 8 * i);
        }
        let pad = 3 - chunk.len();
        for i in 0..4 - pad {
            s.push(ALPH[((n >> (18 - 6 * i)) & 63) as usize] as char);
        }
        for _ in 0..pad {
            s.push('=');
        }
    }
    s
}

fn b64_decode(s: &str) -> Result<Vec<u8>, OrganelleError> {
    let clean: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    if clean.len() % 4 != 0 {
        return Err(OrganelleError::BadInput("bad base64 length".into()));
    }
    let val = |c: char| -> Result<u32, OrganelleError> {
        match c {
            'A'..='Z' => Ok(c as u32 - 'A' as u32),
            'a'..='z' => Ok(c as u32 - 'a' as u32 + 26),
            '0'..='9' => Ok(c as u32 - '0' as u32 + 52),
            '+' => Ok(62),
            '/' => Ok(63),
            '=' => Ok(0),
            _ => Err(OrganelleError::BadInput("bad base64 char".into())),
        }
    };
    let mut out = Vec::new();
    let chars: Vec<char> = clean.chars().collect();
    for chunk in chars.chunks(4) {
        let mut n: u32 = 0;
        let mut pad = 0;
        for (i, &c) in chunk.iter().enumerate() {
            if c == '=' {
                pad += 1;
            }
            n |= val(c)? << (18 - 6 * i);
        }
        for i in 0..3 - pad {
            out.push(((n >> (16 - 8 * i)) & 0xFF) as u8);
        }
    }
    Ok(out)
}

impl Organelle for Archive {
    fn name(&self) -> &'static str {
        "psw-archive"
    }
    fn manifest(&self) -> OrganelleManifest {
        manifest()
    }
    fn call(&self, contract: &str, input: Value) -> Result<Value, OrganelleError> {
        match contract {
            "put" => {
                let b64 = input.get("bytes_b64").and_then(|v| v.as_str()).ok_or_else(|| OrganelleError::BadInput("bytes_b64".into()))?;
                let bytes = b64_decode(b64)?;
                let id = self.put(&bytes).map_err(|e| OrganelleError::Execution(e.to_string()))?;
                Ok(serde_json::json!({"id": id}))
            }
            "get" => {
                let id = input.get("id").and_then(|v| v.as_str()).ok_or_else(|| OrganelleError::BadInput("id".into()))?;
                let bytes = self.get(id).map_err(|e| OrganelleError::Execution(e.to_string()))?;
                Ok(serde_json::json!({"bytes_b64": b64_encode(&bytes)}))
            }
            "contains" => {
                let id = input.get("id").and_then(|v| v.as_str()).ok_or_else(|| OrganelleError::BadInput("id".into()))?;
                Ok(serde_json::json!({"found": self.contains(id)}))
            }
            _ => Err(OrganelleError::UnknownContract(contract.into(), self.name().into())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn put_get_roundtrip() {
        let a = Archive::new();
        let id = a.put(b"hello").unwrap();
        assert_eq!(a.get(&id).unwrap(), b"hello");
    }

    #[test]
    fn content_addressed_same_bytes_same_id() {
        let a = Archive::new();
        assert_eq!(a.put(b"x").unwrap(), a.put(b"x").unwrap());
    }

    #[test]
    fn different_bytes_different_id() {
        let a = Archive::new();
        assert_ne!(a.put(b"x").unwrap(), a.put(b"y").unwrap());
    }

    #[test]
    fn missing_id_errors() {
        let a = Archive::new();
        assert!(matches!(a.get(&"0".repeat(128)), Err(ArchiveError::NotFound(_))));
    }

    #[test]
    fn empty_payload_refused() {
        let a = Archive::new();
        assert!(matches!(a.put(b""), Err(ArchiveError::EmptyPayload)));
    }

    #[test]
    fn b64_roundtrip() {
        let bytes: Vec<u8> = (0..255u8).collect();
        assert_eq!(b64_decode(&b64_encode(&bytes)).unwrap(), bytes);
    }

    #[test]
    fn organelle_put_get_contracts() {
        let a = Archive::new();
        let out = a.call("put", serde_json::json!({"bytes_b64": b64_encode(b"data")})).unwrap();
        let id = out["id"].as_str().unwrap();
        let got = a.call("get", serde_json::json!({"id": id})).unwrap();
        assert_eq!(b64_decode(got["bytes_b64"].as_str().unwrap()).unwrap(), b"data");
    }

    #[test]
    fn organelle_contains_contract() {
        let a = Archive::new();
        let out = a.call("contains", serde_json::json!({"id": "nope"})).unwrap();
        assert_eq!(out["found"], false);
    }
}
