# AUDIT-APPLICATION.md

| Field | Value |
|---|---|
| Repo | ~/workspace/park-send-wreck |
| Date | 2026-10-10 |
| Auditor | brutal-audit rollout slice 7/8 (parent orchestrator) |
| Language | RUST |

## Disposition table

| ID | Rigor finding | Status | Notes |
|---|---|---|---|
| R-1 | Fix-verification claims instance-only until re-attack sign-off | PARTIAL | Exact reproducers fixed; class scope UNKNOWN. No PROVEN/FIXED class claim allowed without new re-attack wave. |
| R-2 | Honest bounds on guardrails; freeze hardening past bound | PARTIAL | psw-redteam bounds only partially reported. Hardening frozen past documented 10-attack coverage. |
| R-3 | Hash-chained log out-of-band head | N/A | No hash-chained log. Receipts are plain JSON; not tamper-evident. |
| R-4 | Triage backlogs; budgets or wont-fix decisions | PARTIAL | Known-limitations queue is backlog; items QUEUED, no budgets or wont-fix recorded. |
| R-5 | Signature/denylist/strip games best-effort | N/A | No such control present. |
| R-6 | Detector must have named consumer + enforcement gate | N/A | psw-redteam is a test harness, not a deployed detector. |
| R-7 | Property vs change-detection test separation | PARTIAL | 167/167 at recorded commit; labeling UNKNOWN. Policy now required. |
| R-8 | Unsigned provenance declared honestly | CONFORMANT | receipts/PROVENANCE.md added today. |
| R-9 | Claim log frozen schema / SUPERSEDED-BY-BYPASS | N/A | No claim log present. |
| R-10 | Record-keeping same motion as event | CONFORMANT | Receipts include ts_utc, actor, verify command. |
| R-11 | Explicit UNKNOWN list; no “probably” as PROVEN | CONFORMANT | UNKNOWN list included below. |
| R-12 | No unsourced oral footnotes | CONFORMANT | Receipt verify fields and report campaign references are evidence-based. |

## R-1 — Fix-verification claims are instance-only

**Label:** PROVEN for exact reproducers; UNKNOWN for class scope.

The two findings listed as fixed during the campaign are fixed only against the exact attack inputs. No `PROVEN` or `FIXED` class-wide claim is accepted without a dedicated re-attack wave using new variants and a sign-off recorded in this document.

Claims and defeats remain in one book: `docs/REDTEAM-REPORT.md`. The `Known limitations (not fixed; queued)` section is the explicit backlog.

## R-2 — Honest bounds; freeze past documented bound

**Label:** PARTIAL.

The red-team campaign covered 10 attacks. The covered/not-covered matrix is only partially stated in the report. Hardening claims past that documented 10-attack bound are frozen until a new bound is written and tested.

## R-3 — Hash-chained log / tamper-evident limitation

**Label:** N/A.

No hash-chained log exists. `receipts/` are plain JSON, not tamper-evident. No receipt may be described as tamper-evident. The same-machine/same-VM limitation does not need an out-of-band head until a hash-chain log is introduced.

## R-4 — Backlog triage

**Label:** PARTIAL.

The `Known limitations (not fixed; queued)` section of `docs/REDTEAM-REPORT.md` is the backlog. Each item is `QUEUED` pending a complexity budget or a wont-fix decision. Standing HIGHs must not remain without one. Unbounded queue growth is the failure mode.

See “Wont-fix / backlog triage” below.

## R-5 — Signature/denylist/strip games

**Label:** N/A.

No signature, denylist, or strip game control is currently in use. If introduced, best-effort games get a fixed complexity budget and structural answers are preferred.

## R-6 — Detector / enforcement gate

**Label:** N/A as a deployed control.

`psw-redteam` is a test harness. Its consumer is the report and the resulting fixes. If it is ever promoted to an enforcement control, it must have a named consumer and an enforcement gate, or it must be deleted.

## R-7 — Property tests vs change-detection tests

**Label:** PARTIAL.

Test count: `167/167` at recorded 2026-10-07 commit. The split between property tests and change-detection tests is UNKNOWN.

Policy:
- Security-invariant property tests must be explicitly labeled.
- Property test expectations must never be rewritten to match a behavior change unless an adversarial sign-off is recorded.
- Change-detection tests may be updated in the ordinary flow.

## R-8 — Unsigned provenance

**Label:** PROVEN for conformance.

`receipts/PROVENANCE.md` was added today. It declares all receipts honestly-unsigned, states that reproduction via the named verify command is the verification, and records the gap that there is no commit-hash binding. If unforgeability is ever required, the existing `lucidota-receipt/1` envelope is the path, not a new format.

## R-9 — Claim log schema

**Label:** N/A.

No claim log exists. If introduced, the schema must be frozen to enum verdicts only and must support `SUPERSEDED-BY-BYPASS`.

## R-10 — Record-keeping same motion

**Label:** PROVEN for conformance.

Receipts contain `ts_utc`, actor/device, and verify command, and were written in the same motion as the builds.

## R-11 — UNKNOWN list

**Label:** PROVEN for explicit listing; UNKNOWN for the listed items.

No `probably` is treated as `PROVEN`.

Explicit UNKNOWN items:

1. Full class scope of the two fixed findings beyond their exact reproducers.
2. Current validity of the queued `Known limitations` items today.
3. Whether the original 10 attacks in the red-team campaign cover the current tree; the tree has moved since.
4. Which of the 167 tests are property tests vs change-detection tests.

## R-12 — Unsourced oral footnotes

**Label:** PROVEN for conformance.

Receipt `verify` fields name the reproduction command. Report attack claims reference the campaign. Anything without evidence is `UNVERIFIED`.

## Changes made today

| File | Change | Conformance |
|---|---|---|
| `receipts/PROVENANCE.md` | R-8 application. Declares receipts honestly-unsigned; no crypto proof; verification power is reproduction via named verify command; receipt is the pointer; no commit-hash binding recorded gap; unforgeability path is `lucidota-receipt/1`, not a new format. | CONFORMANT |

## Commits

| Scope | Status |
|---|---|
| Recorded 2026-10-07 close-shit commit used for test count | `2fc5654` (pre-existing, recorded-not-rerun) |
| `receipts/PROVENANCE.md` addition 2026-10-10 | `fa07a25` |
| Any other current-tree commits | none in this pass |

## Test counts

| Source | Count | Status |
|---|---|---|
| 2026-10-07 close-shit commit | 167/167 | recorded-not-rerun; not re-run in this pass |

## Wont-fix / backlog triage

All items in `docs/REDTEAM-REPORT.md` under `Known limitations (not fixed; queued)` are:

- `QUEUED` pending triage.
- No complexity budget or wont-fix decision has been recorded.
- Standing HIGHs without a budget or wont-fix decision are not closed.

Failure mode: unbounded queue growth.
