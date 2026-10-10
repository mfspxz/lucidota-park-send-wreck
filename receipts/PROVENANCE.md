# receipts/ — PROVENANCE (R-8)

**Date:** 2026-10-10. **Status:** honestly-unsigned.

All `*.json` files in this directory are **unsigned build receipts**. They
carry: the item/crate built, UTC timestamp, actor/device/chat attribution,
test counts (measured), a `verify` field naming the reproduction command,
and an `epistemic` field (e.g. `RECONSTRUCTION`).

**Verification power, stated plainly:**
- They prove NOTHING cryptographically. A forgeable-by-writer claim file.
- What they DO provide: structured, attributed build evidence that any
  reader can re-check by running the named `verify` command
  (`cargo test --workspace`) against the committed tree at the recorded
  commit. Reproduction is the verification; the receipt is the pointer.
- They do not bind to a commit hash (no `git rev-parse` recorded) — a gap.
  Binding receipts to commits is queued, not done.

No `signature` field exists on any receipt here. Nothing in this
directory should be read as signed provenance. If a consumer ever needs
unforgeability, the path is the existing `lucidota-receipt/1` envelope
(see `~/workspace/bin/sign-receipt.sh` + `verify-receipt.sh`) — not a new
format invented here.
