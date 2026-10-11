# park-send-wreck — organelle mapping decisions

Wave 4b-1, corrected doctrine. The repo already had its own organelle
substrate (`pw-organelles` trait + `pw-host` + `pw-manifest`, Aug 2026
tree): verified, not rebuilt. The 15 capability units below are re-expressed
as v1-standard organelles (codons, capability contracts, hash-chained
receipts via the standard host, kill semantics); the pw substrate stays as
the repo's internal execution layer (noted, not an organelle — a host is
not a capability unit).

## Capability units found

| Unit | Source | Ops | Decision |
|---|---|---|---|
| psw-workbench | `psw-workbench::Workbench::compose` | compose | **Bespoke** (`wb`). Validated flow-DAG composition: unknown organelles, undeclared contracts, cycles all refused before execution. Proven (deterministic). |
| psw-lab | `psw-lab::Lab` | preregister, run, runs_for | **Bespoke** (`lb`). Pre-registered experiments; runs recorded against immutable pre-regs. Candidate. |
| psw-forge | `psw-forge::Forge` | build, verify | **Bespoke** (`fg`). Deterministic artifact builder (same spec → same bytes → same blake2 id; determinism asserted in tests). Proven. |
| psw-archive | `psw-archive::Archive` | put, get, contains, len | **Bespoke** (`ar`). Considered ref `file-cas` (`fc`): that is a FILESYSTEM content-addressed store; this is an IN-MEMORY blake2-512 vault (restarts lose blobs). Same addressing idea, different persistence contract — bespoke, noted as the conceptual match. Candidate. |
| psw-arts-crafts | `psw-arts-crafts::ArtsCrafts` | render_map, render_receipt_count | **Bespoke** (`ac`). Grid state → SVG maps. Proven (pure rendering). |
| psw-cockpit | `psw-cockpit::Cockpit` | plan_release, start_stage, finish_stage, status | **Bespoke** (`qk`). Named release stage sequences. Candidate. |
| psw-court | `psw-court::Court` | file, adjudicate, rulings_for | **Bespoke** (`qt`). Claim adjudication over supports/refutes/neutral evidence. Candidate. |
| psw-observatory | `psw-observatory::Observatory` | emit, query, tail, count | **Bespoke** (`ob`). Considered ref `ledger-append` (`la`): that appends canonical JSON lines to a file; this is an in-memory TELEMETRY stream with kind-filtered queries and tails. Different read contract — bespoke. Candidate. |
| psw-parity | `psw-parity::Parity::check` | check | **Bespoke** (`py`). Transport-equivalence checking — the exact property the v1 standard demands ("equivalence-tested across transports"). Proven (pure digest comparison). |
| psw-pipeline | `psw-pipeline::Pipeline::run` | run | **Bespoke** (`pp`). Discover→Archive full loop. Write. Candidate. |
| psw-quarantine | `psw-quarantine::Quarantine` | inspect, contain | **Bespoke** (`qn`). Risk scoring + containment decisions for untrusted payloads; nothing untrusted executes. inspect is ReadOnly; contain is Write. Candidate. |
| psw-reconstruction | `psw-reconstruction::verify_byte_proof` | verify_proof, crate_inventory | **Bespoke** (`rc`). Forensic byte-proof scan of the lost Aug 2026 tree. ReadOnly. Candidate (byte-proof of THIS tree not established — honest valence). |
| pw-grid | `pw-grid::Grid` | insert, query_box, len | **Bespoke** (`gr`). RTS/GIS entity grid with authority tiers; quarantined entities never enter selections. Candidate. |
| pw-plugin | `pw-plugin::PluginLoader` | discover, load_all | **Bespoke** (`qp`). Manifest-driven in-process plugin loading (v1; cdylib explicitly future). ExternalEffect. Candidate. |

## Deliberate exclusions

- `pw-organelles` / `pw-manifest` / `pw-host`: the repo's own organelle
  substrate (trait, manifest v1, host with receipts). Verified present and
  tested; the migration registers the units above on the STANDARD host —
  the pw substrate is not rebuilt and not wrapped as an organelle.
- `pw-domain`: types + trinary epistemics. Type library, not a capability.
- `psw-redteam`: attack harness — `Probe` is a test mock and the attacks are
  `#[test]` fns, not independently callable capabilities (same reasoning as
  the pilot's `bench` exclusion). Its tests stay green untouched.

## Reference-organelle check (all 12 considered, 2 near-misses, 0 adoptions)

- `ref-file-cas` vs psw-archive: in-memory vs filesystem CAS (above).
- `ref-ledger-append` vs psw-observatory: file JSONL append vs in-memory
  telemetry stream with queries (above).
- Others: no overlap.
- Verdict: 14 bespoke organelles, 0 adoptions. The near-misses were
  genuinely different contracts, not reinventions.

## Execution facts

- 4 Proven (workbench, forge, arts-crafts, parity), 10 Candidate.
- Write/ExternalEffect units carry `psw.*` capabilities; host fail-closes
  without them (capability-gated test contexts throughout).
- `cargo test --workspace`: all suites green. Every organelle test proves
  register → call → verify_chain.
