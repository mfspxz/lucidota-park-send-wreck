//! psw-11 — Reconstruction from proven hashes.
//! The Aug 2026 tree is lost (S22 stolen, no surviving repo/zip). What
//! survives is PROVEN metadata from Zackary's pasted records (ledger seq
//! 265, Aug 24 ChatGPT chat "Recreate Parks End Wreck"):
//!   - source snapshot ParkSendWreck_FULL_SOURCE_20260816T202218Z.zip, SHA-256
//!   - proven HEAD 4542dfd937a0697f01b8ef70834aaf5e91b545f8
//!   - proven APK + Linux binary SHA-256s
//!   - branch feat/workbench-ontological-execution
//!   - crates: parkwreck-domain, parkwreck-grid, parkwreck-organelles,
//!     parkwreck-plugin
//!   - named failure SEL-BOX-ADD
//! This crate is the evidence registry: it records those hashes as
//! constants, scans for byte-proof (the actual artifacts), and reports
//! the honest trinary status. It never claims byte-proof it does not have.
use pw_domain::{hash_bytes_hex, EpistemicStatus};
use serde::{Deserialize, Serialize};

/// Proven: source snapshot ZIP SHA-256 (Zackary's pasted record, seq 265).
pub const PROVEN_ZIP_SHA256: &str = "a2d0524d4dd22e19624fbfac3781505407f21d80abc716d8e322f7647d02b4bb";
/// Proven: git HEAD of the Aug 2026 tree.
pub const PROVEN_HEAD: &str = "4542dfd937a0697f01b8ef70834aaf5e91b545f8";
/// Proven: release APK SHA-256.
pub const PROVEN_APK_SHA256: &str = "a8c0aa026df270031a9427531a45cd222f3b218d7189c029f56b9d0eee7ff5a3";
/// Proven: Linux binary SHA-256.
pub const PROVEN_LINUX_SHA256: &str = "0f024fe398f5243e64f42fab00240bd94848046262e002ee12463624add07182";
/// Proven: feature branch name.
pub const PROVEN_BRANCH: &str = "feat/workbench-ontological-execution";
/// Proven: original workspace path on his machine.
pub const PROVEN_WORKSPACE: &str = "/home/mfspx/LUCIDOTA/S_H_A_M_B_A_L_A";
/// Proven: crate names in the Aug 2026 tree.
pub const PROVEN_CRATES: &[&str] = &[
    "parkwreck-domain",
    "parkwreck-grid",
    "parkwreck-organelles",
    "parkwreck-plugin",
];
/// Proven: research dossier filename.
pub const PROVEN_DOSSIER: &str = "PARKSENDWRECK_RTS_GIS_INTERACTION_DEEP_RESEARCH_20260822.md";
/// Named failure. Original semantics UNVERIFIED (name only).
pub const NAMED_FAILURE: &str = "SEL-BOX-ADD";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ReconstructionStatus {
    /// Byte-proof found and hashes match: the tree is restored, not rebuilt.
    ByteProven,
    /// Hashes known, bytes missing: faithful reconstruction from records.
    Reconstruction,
    /// No hashes, no bytes.
    Missing,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactCheck {
    pub name: String,
    pub expected_sha256: String,
    pub found_path: Option<String>,
    pub matches: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProofReport {
    pub status: ReconstructionStatus,
    pub epistemic: EpistemicStatus,
    pub checks: Vec<ArtifactCheck>,
    pub honest_gaps: Vec<String>,
}

/// Scan `search_dirs` (recursive, bounded) for the proven artifacts.
/// Compares SHA-256 of any candidate file against the proven hashes.
pub fn verify_byte_proof(search_dirs: &[&std::path::Path]) -> ProofReport {
    let mut checks = vec![
        ArtifactCheck { name: "source-zip".into(), expected_sha256: PROVEN_ZIP_SHA256.into(), found_path: None, matches: false },
        ArtifactCheck { name: "apk".into(), expected_sha256: PROVEN_APK_SHA256.into(), found_path: None, matches: false },
        ArtifactCheck { name: "linux-binary".into(), expected_sha256: PROVEN_LINUX_SHA256.into(), found_path: None, matches: false },
    ];
    let mut scanned = 0usize;
    for dir in search_dirs {
        scan_dir(dir, &mut checks, &mut scanned, 0);
    }
    let matched = checks.iter().filter(|c| c.matches).count();
    let (status, epistemic) = if matched == checks.len() {
        (ReconstructionStatus::ByteProven, EpistemicStatus::Proven)
    } else if matched == 0 {
        (ReconstructionStatus::Reconstruction, EpistemicStatus::Unknown)
    } else {
        (ReconstructionStatus::Reconstruction, EpistemicStatus::Unknown)
    };
    let mut honest_gaps = vec![
        format!("scanned {scanned} files under {} dir(s); {matched}/{} artifacts byte-matched", search_dirs.len(), checks.len()),
        "claimed commits 06de49f8/89bfde14/804fa63/501c940: MEMORY only, never in a surviving repo (seq 257)".into(),
        "SEL-BOX-ADD original semantics: name only, implementation UNVERIFIED".into(),
        "GO-50 ontology: no surviving artifact (GO-25 locked May 29 in diogenes)".into(),
    ];
    if matched > 0 && matched < checks.len() {
        honest_gaps.push("PARTIAL byte-proof: some artifacts matched, others missing — status stays RECONSTRUCTION".into());
    }
    ProofReport { status, epistemic, checks, honest_gaps }
}

fn scan_dir(dir: &std::path::Path, checks: &mut [ArtifactCheck], scanned: &mut usize, depth: usize) {
    if depth > 6 || *scanned > 50_000 {
        return;
    }
    let rd = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(_) => return,
    };
    for entry in rd.filter_map(|e| e.ok()) {
        let p = entry.path();
        if p.is_dir() {
            // Skip the rebuild itself and heavy dirs.
            if let Some(name) = p.file_name().and_then(|n| n.to_str()) {
                if name == "target" || name == ".git" || name == "node_modules" {
                    continue;
                }
            }
            scan_dir(&p, checks, scanned, depth + 1);
        } else if p.is_file() {
            // Cheap pre-filter: only hash plausible candidates by extension/size.
            let ext_ok = p.extension().and_then(|e| e.to_str()).map(|e| matches!(e, "zip" | "apk")).unwrap_or(false);
            let big_ok = p.metadata().map(|m| m.len() > 1_000_000).unwrap_or(false);
            *scanned += 1;
            if ext_ok || big_ok {
                if let Ok(bytes) = std::fs::read(&p) {
                    let digest = hash_bytes_hex(&bytes);
                    // hash_bytes_hex is blake2-512 (128 hex); proven are SHA-256 (64 hex).
                    // Compare via sha256 computed manually below.
                    let sha = sha256_hex(&bytes);
                    for c in checks.iter_mut() {
                        if !c.matches && sha == c.expected_sha256 {
                            c.matches = true;
                            c.found_path = Some(p.display().to_string());
                        }
                    }
                    let _ = digest;
                }
            }
        }
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    // Minimal SHA-256 (no extra dep): standard implementation.
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
        0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
        0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
        0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
        0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
        0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
        0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
    ];
    let mut h: [u32; 8] = [0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19];
    let mut msg = bytes.to_vec();
    let bitlen = (bytes.len() as u64).wrapping_mul(8);
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bitlen.to_be_bytes());
    for chunk in msg.chunks(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([chunk[4 * i], chunk[4 * i + 1], chunk[4 * i + 2], chunk[4 * i + 3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }
        let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh) =
            (h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]);
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let t1 = hh.wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }
    h.iter().map(|x| format!("{x:08x}")).collect()
}

/// The four proven crate names map 1:1 onto the rebuild's crates.
pub fn crate_inventory() -> Vec<(&'static str, &'static str)> {
    vec![
        ("parkwreck-domain", "pw-domain"),
        ("parkwreck-grid", "pw-grid"),
        ("parkwreck-organelles", "pw-organelles"),
        ("parkwreck-plugin", "pw-plugin"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_known_vector() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn sha256_empty_vector() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn proven_hashes_are_wellformed() {
        for h in [PROVEN_ZIP_SHA256, PROVEN_APK_SHA256, PROVEN_LINUX_SHA256] {
            assert_eq!(h.len(), 64);
            assert!(h.chars().all(|c| c.is_ascii_hexdigit()));
        }
        assert_eq!(PROVEN_HEAD.len(), 40);
    }

    #[test]
    fn crate_inventory_covers_all_proven() {
        let inv = crate_inventory();
        assert_eq!(inv.len(), 4);
        for proven in PROVEN_CRATES {
            assert!(inv.iter().any(|(p, _)| p == proven), "missing {proven}");
        }
    }

    #[test]
    fn no_byte_proof_in_empty_dir() {
        let dir = std::env::temp_dir().join("psw-recon-empty");
        std::fs::create_dir_all(&dir).unwrap();
        let report = verify_byte_proof(&[dir.as_path()]);
        assert_eq!(report.status, ReconstructionStatus::Reconstruction);
        assert_eq!(report.epistemic, EpistemicStatus::Unknown);
        assert!(report.checks.iter().all(|c| !c.matches));
        assert!(!report.honest_gaps.is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn detects_planted_byte_proof() {
        // Plant a file whose SHA-256 we claim is proven, then verify the
        // scanner actually matches bytes (proves the check is real).
        let dir = std::env::temp_dir().join("psw-recon-plant");
        std::fs::create_dir_all(&dir).unwrap();
        let payload = b"planted-artifact-bytes";
        std::fs::write(dir.join("artifact.zip"), payload).unwrap();
        let expected = sha256_hex(payload);
        // Direct check of the matching logic via a synthetic report path:
        let mut checks = vec![ArtifactCheck {
            name: "planted".into(),
            expected_sha256: expected,
            found_path: None,
            matches: false,
        }];
        let mut scanned = 0;
        scan_dir(dir.as_path(), &mut checks, &mut scanned, 0);
        assert!(checks[0].matches);
        assert!(checks[0].found_path.is_some());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn named_failure_recorded() {
        assert_eq!(NAMED_FAILURE, "SEL-BOX-ADD");
        assert_eq!(PROVEN_BRANCH, "feat/workbench-ontological-execution");
    }
}
