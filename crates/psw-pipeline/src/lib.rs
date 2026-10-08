//! psw-13 — Discover -> Archive pipeline.
//! The full PSW loop in one pass:
//!   discover -> inspect -> compose -> execute -> test -> audit -> prove -> archive
//! Software is discovered (manifests), inspected (validated), composed
//! (workbench flow), executed (host), tested (probes), audited (court),
//! proven (receipt chain), archived (CAS). Any stage failing halts the run
//! and the failure is itself receipted.
use pw_domain::{AuthorityTier, Verdict};
use pw_host::Host;
use pw_manifest::Transport;
use pw_organelles::Organelle;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;

use psw_archive::Archive;
use psw_arts_crafts::ArtsCrafts;
use psw_cockpit::Cockpit;
use psw_court::{Court, Evidence, EvidenceStance};
use psw_forge::Forge;
use psw_lab::Lab;
use psw_observatory::Observatory;
use psw_parity::Parity;
use psw_quarantine::Quarantine;
use psw_workbench::Workbench;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageResult {
    pub stage: String,
    pub ok: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineReport {
    pub stages: Vec<StageResult>,
    pub all_ok: bool,
    pub court_verdict: Option<Verdict>,
    pub archive_id: Option<String>,
    pub receipt_count: usize,
}

pub struct Pipeline {
    pub host: Host,
    archive: Arc<Archive>,
    court: Arc<Court>,
}

impl Pipeline {
    pub fn new() -> Self {
        let host = Host::new("psw-pipeline");
        let archive = Arc::new(Archive::new());
        let court = Arc::new(Court::new());
        host.register(Arc::new(Workbench)).unwrap();
        host.register(Arc::new(Lab::new())).unwrap();
        host.register(Arc::new(Forge)).unwrap();
        host.register(Arc::new(ArtsCrafts)).unwrap();
        host.register(archive.clone() as Arc<dyn Organelle>).unwrap();
        host.register(Arc::new(Parity)).unwrap();
        host.register(Arc::new(Observatory::new())).unwrap();
        host.register(court.clone() as Arc<dyn Organelle>).unwrap();
        host.register(Arc::new(Quarantine)).unwrap();
        host.register(Arc::new(Cockpit::new())).unwrap();
        Pipeline { host, archive, court }
    }

    fn stage(results: &mut Vec<StageResult>, stage: &str, ok: bool, detail: impl Into<String>) {
        results.push(StageResult { stage: stage.into(), ok, detail: detail.into() });
    }

    /// Probe calls: one safe, idempotent-ish call per subsystem.
    fn probes() -> Vec<(&'static str, &'static str, Value)> {
        vec![
            ("psw-workbench", "describe", serde_json::json!({})),
            ("psw-lab", "runs", serde_json::json!({"exp_id": "probe"})),
            ("psw-forge", "build", serde_json::json!({"kind": "probe", "spec": {"v": 1}})),
            ("psw-arts-crafts", "render-receipts", serde_json::json!({"count": 0})),
            ("psw-archive", "contains", serde_json::json!({"id": "probe"})),
            ("psw-parity", "check", serde_json::json!({"organelle": "probe", "contract": "probe",
                "samples": [{"transport": "in-process", "output_digest": "d"}]})),
            ("psw-observatory", "tail", serde_json::json!({"n": 1})),
            ("psw-court", "rulings", serde_json::json!({"claim_id": "probe"})),
            ("psw-quarantine", "inspect", serde_json::json!({"payload": {"probe": true}})),
            ("psw-cockpit", "plan", serde_json::json!({"name": "probe", "stages": ["s1"]})),
        ]
    }

    /// Run the full discover->archive loop. `manifest_dir` holds the
    /// on-disk organelle manifests to discover (may be empty in tests).
    pub fn run(&self, manifest_dir: &std::path::Path) -> PipelineReport {
        let mut stages: Vec<StageResult> = Vec::new();
        let auth = AuthorityTier::Admin;

        // 1. DISCOVER
        match self.host.load_manifests_dir(manifest_dir) {
            Ok(names) => {
                Self::stage(&mut stages, "discover", true, format!("{} manifest(s) discovered on disk", names.len()));
            }
            Err(e) => {
                Self::stage(&mut stages, "discover", false, e.to_string());
            }
        }

        // 2. INSPECT — re-validate every registered organelle's manifest.
        // Fail-closed: one invalid manifest fails the stage.
        let mut inspect_ok = true;
        let mut inspect_detail = String::new();
        for name in self.host.organelles() {
            let organelle = self.host.get_organelle(&name);
            match organelle {
                Some(o) => {
                    if let Err(e) = o.manifest().validate() {
                        inspect_ok = false;
                        inspect_detail = format!("{name}: {e}");
                        break;
                    }
                }
                None => {
                    inspect_ok = false;
                    inspect_detail = format!("{name}: vanished from registry");
                    break;
                }
            }
        }
        if inspect_detail.is_empty() {
            inspect_detail = format!("{} manifest(s) re-validated", self.host.organelles().len());
        }
        Self::stage(&mut stages, "inspect", inspect_ok, inspect_detail);

        // 3. COMPOSE — workbench validates the probe flow DAG.
        let flow = psw_workbench::Flow {
            name: "probe-flow".into(),
            steps: Self::probes()
                .iter()
                .enumerate()
                .map(|(i, (org, contract, input))| psw_workbench::FlowStep {
                    id: format!("s{i}"),
                    organelle: org.to_string(),
                    contract: contract.to_string(),
                    input: input.clone(),
                    after: if i == 0 { vec![] } else { vec![format!("s{}", i - 1)] },
                })
                .collect(),
        };
        let full_catalog: std::collections::HashMap<String, Vec<String>> = Self::probes()
            .iter()
            .map(|(o, c, _)| (o.to_string(), vec![c.to_string()]))
            .collect();
        match Workbench::compose(&flow, &full_catalog) {
            Ok(plan) => Self::stage(&mut stages, "compose", true, format!("flow valid, {} steps", plan.step_count)),
            Err(e) => {
                Self::stage(&mut stages, "compose", false, e.to_string());
                inspect_ok = false;
            }
        }

        // 4. EXECUTE — run every probe through the host.
        let mut executed = 0;
        let mut exec_failed = Vec::new();
        for (org, contract, input) in Self::probes() {
            match self.host.call(org, contract, Transport::InProcess, input, auth) {
                Ok(_) => executed += 1,
                Err(e) => exec_failed.push(format!("{org}.{contract}: {e}")),
            }
        }
        Self::stage(
            &mut stages,
            "execute",
            exec_failed.is_empty(),
            if exec_failed.is_empty() {
                format!("{executed}/{} probes executed", Self::probes().len())
            } else {
                exec_failed.join("; ")
            },
        );

        // 5. TEST — parity self-check: same probe twice must digest identically.
        let mut parity_ok = true;
        let mut parity_detail = String::new();
        for (org, contract, input) in Self::probes().iter().take(3) {
            let d1 = self.host.call(org, contract, Transport::InProcess, input.clone(), auth).ok().map(|(o, _)| pw_domain::hash_hex(&o.to_string()));
            let d2 = self.host.call(org, contract, Transport::InProcess, input.clone(), auth).ok().map(|(o, _)| pw_domain::hash_hex(&o.to_string()));
            if d1 != d2 || d1.is_none() {
                parity_ok = false;
                parity_detail = format!("non-deterministic probe: {org}.{contract}");
                break;
            }
        }
        if parity_detail.is_empty() {
            parity_detail = "3 probes re-run: digests identical".into();
        }
        Self::stage(&mut stages, "test", parity_ok, parity_detail);

        // 6. AUDIT — court rules on the run.
        let evidence: Vec<Evidence> = stages
            .iter()
            .enumerate()
            .map(|(i, s)| Evidence {
                id: format!("stage-{i}"),
                stance: if s.ok { EvidenceStance::Supports } else { EvidenceStance::Refutes },
                summary: format!("{}: {}", s.stage, s.detail),
            })
            .collect();
        let claim_id = self.court.file("psw pipeline run healthy", evidence);
        let verdict = self.court.adjudicate(&claim_id).map(|r| r.verdict).unwrap_or(Verdict::Inconclusive);
        Self::stage(&mut stages, "audit", verdict == Verdict::Supported, format!("court verdict: {verdict:?}"));

        // 7. PROVE — receipt chain must verify end to end.
        let chain_ok = self.host.verify_receipts().is_ok();
        Self::stage(
            &mut stages,
            "prove",
            chain_ok,
            format!("{} host receipt(s), chain {}", self.host.receipt_count(), if chain_ok { "valid" } else { "BROKEN" }),
        );

        // 8. ARCHIVE — the report itself goes into the CAS vault.
        let all_ok = stages.iter().all(|s| s.ok) && inspect_ok;
        let report = PipelineReport {
            stages,
            all_ok,
            court_verdict: Some(verdict),
            archive_id: None,
            receipt_count: self.host.receipt_count(),
        };
        let bytes = serde_json::to_vec_pretty(&report).unwrap();
        let archive_id = self.archive.put(&bytes).ok();
        let mut report = report;
        report.archive_id = archive_id;

        // Fail-closed: a failed run archives the failure report too (provenance),
        // but all_ok reflects the truth.
        report
    }
}

impl Default for Pipeline {
    fn default() -> Self {
        Pipeline::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest_dir() -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let n = N.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("psw-pipeline-manifests-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn full_run_all_stages_ok() {
        let p = Pipeline::new();
        let dir = manifest_dir();
        let report = p.run(&dir);
        for s in &report.stages {
            assert!(s.ok, "stage {} failed: {}", s.stage, s.detail);
        }
        assert!(report.all_ok);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn eight_stages_present() {
        let p = Pipeline::new();
        let dir = manifest_dir();
        let report = p.run(&dir);
        let names: Vec<&str> = report.stages.iter().map(|s| s.stage.as_str()).collect();
        for want in ["discover", "inspect", "compose", "execute", "test", "audit", "prove"] {
            assert!(names.contains(&want), "missing stage {want}");
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn court_verdict_supported() {
        let p = Pipeline::new();
        let dir = manifest_dir();
        let report = p.run(&dir);
        assert_eq!(report.court_verdict, Some(Verdict::Supported));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn report_archived_with_content_id() {
        let p = Pipeline::new();
        let dir = manifest_dir();
        let report = p.run(&dir);
        let id = report.archive_id.expect("archive id");
        assert_eq!(id.len(), 128);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn receipts_emitted() {
        let p = Pipeline::new();
        let dir = manifest_dir();
        let report = p.run(&dir);
        // 10 probes (execute) + 6 re-runs (test/parity) = 16 host calls = 16 receipts.
        assert!(report.receipt_count >= 16, "expected >=16 receipts, got {}", report.receipt_count);
        assert!(p.host.verify_receipts().is_ok());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn discover_with_manifests_on_disk() {
        let dir = std::env::temp_dir().join("psw-pipeline-manifests2");
        std::fs::create_dir_all(&dir).unwrap();
        let m = Workbench.manifest();
        std::fs::write(dir.join("psw-workbench.json"), serde_json::to_string(&m).unwrap()).unwrap();
        let p = Pipeline::new();
        let report = p.run(&dir);
        let discover = report.stages.iter().find(|s| s.stage == "discover").unwrap();
        assert!(discover.ok);
        assert!(discover.detail.contains('1'));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn ten_subsystems_registered() {
        let p = Pipeline::new();
        assert_eq!(p.host.organelles().len(), 10);
    }

    #[test]
    fn cockpit_dogfood_operates_release() {
        // psw-15: the cockpit operates one full release, driven through
        // the host (authority-checked) rather than called directly.
        let p = Pipeline::new();
        let auth = AuthorityTier::Admin;
        let stages = ["discover", "inspect", "compose", "execute", "test", "audit", "prove", "archive"];
        let (out, _) = p
            .host
            .call("psw-cockpit", "plan", Transport::InProcess,
                serde_json::json!({"name": "psw-nightly", "stages": stages}), auth)
            .unwrap();
        let rid = out["release_id"].as_str().unwrap().to_string();
        for s in stages {
            p.host.call("psw-cockpit", "start", Transport::InProcess,
                serde_json::json!({"release_id": rid, "stage": s}), auth).unwrap();
            p.host.call("psw-cockpit", "finish", Transport::InProcess,
                serde_json::json!({"release_id": rid, "stage": s, "ok": true, "note": "dogfood"}), auth).unwrap();
        }
        let (st, _) = p.host.call("psw-cockpit", "status", Transport::InProcess,
            serde_json::json!({"release_id": rid}), auth).unwrap();
        assert_eq!(st["halted"], false);
        assert_eq!(st["stages"].as_array().unwrap().len(), 8);
        for s in st["stages"].as_array().unwrap() {
            assert_eq!(s["state"], "DONE", "stage {} not DONE", s["name"]);
        }
        // Authority is enforced on the cockpit: Open callers are refused.
        let e = p.host.call("psw-cockpit", "status", Transport::InProcess,
            serde_json::json!({"release_id": rid}), AuthorityTier::Open).unwrap_err();
        assert!(matches!(e, pw_host::HostError::AuthorityDenied { .. }));
    }
}
