# PSW Documentation Canon (psw-18)

Three sources. Every claim about ParkSendWreck must be traceable to one of
them, and the sources are ranked: when they disagree, the higher source wins.

## The three sources

1. **HARNESS** (highest) — the running system itself: `cargo test --workspace`
   output, receipts in `receipts/`, the pipeline's `PipelineReport`.
   If the harness says a test fails, no document overrules it.
2. **USER** — Zackary's records: ledger seq 265 (proven hashes, branch,
   crate names, SEL-BOX-ADD name), the dossier, his pasted ChatGPT records.
   The user's pasted records are provenance for *names and hashes*, never
   for *behavior*.
3. **DOCS** (lowest) — this directory, comments, READMEs. Docs describe
   intent. When docs and harness disagree, the harness is right and the
   docs are stale.

## Trinary labels (mandatory on every claim)

- **PROVEN** — demonstrated by the harness or by byte-matching a proven hash.
- **RECONSTRUCTION** — faithfully rebuilt from USER records; behaviorally
  plausible, byte-unverified.
- **DESIGNED** — invented in this rebuild (subsystem semantics, court rule
  thresholds, quarantine signal lists).
- **UNKNOWN** — recorded as unknown. Never collapsed to true/false.

## Audit procedure (for the next auditor)

1. `cargo test --workspace` — must be green. Note the per-crate counts.
2. `cargo test -p psw-redteam` — all 10 attacks must fail closed.
3. Run the pipeline: the `PipelineReport` must show all 8 stages ok,
   court verdict SUPPORTED, receipt chain valid.
4. Spot-check `receipts/*.json` against the harness: every receipt's
   `test_counts` must match a real test run you just performed.
5. Verify the reconstruction registry: `psw-reconstruction` must report
   status RECONSTRUCTION (never BYTE_PROVEN) until a byte-proof artifact
   is found and its SHA-256 matches a proven hash.
6. Read `docs/REDTEAM-REPORT.md` — confirm every finding is either fixed
   (with the fixing commit/test named) or recorded as a known limitation
   with a queued follow-up.

## What is NOT claimed

- The Aug 2026 source is NOT restored. Hashes are known; bytes are missing.
- Subsystem behaviors (court thresholds, quarantine signals, forge framing)
  are DESIGNED, not recovered.
- SEL-BOX-ADD's original trigger is UNKNOWN; the reconstructed failure
  class (unfiltered box-select-add) is documented in `pw-grid`.
