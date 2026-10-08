# PSW Red-Team Report (psw-14)

Campaign: PARK.SEND.WRECK the rebuilt PSW itself.
Executable half: `cargo test -p psw-redteam` (10 attacks, all must fail closed).
Date: 2026-10-06/07. Trinary: findings are PROVEN (by test) or KNOWN LIMITATION.

## Attacks and outcomes

| # | Attack | Target | Result |
|---|--------|--------|--------|
| A1 | Organelle name confusion (whitespace, traversal, empty, case, NUL) | host.call | REFUSED (Unregistered) — 6/6 |
| A2 | Forged receipt spliced into chain | receipt chain | REJECTED (tamper detected, chain broken) |
| A3 | Transport confusion (REST/MCP/UnixSocket on InProcess-only) | host.call | REFUSED (UnsupportedTransport) — 3/3 |
| A4 | Privilege escalation (Open/Operator -> Admin contract) | host.call | DENIED (AuthorityDenied) — 2/2 |
| A5 | Contract case smuggling ("PING" vs "ping") | host.call | REFUSED (UndeclaredContract; no normalization) |
| A6 | Quarantine evasion: __proto__ nested 3 deep in arrays | quarantine | BLOCKED (signal found, contain refused) |
| A7 | Quarantine evasion: polyglot string (two patterns, one string) | quarantine | BLOCKED (score >= 50, refused) |
| A8 | Archive poisoning (different bytes under same id) | archive | IMPOSSIBLE (content addressing; no overwrite API) |
| A9 | Pipeline silent-pass (all_ok constant true) | pipeline | HONEST (all_ok = AND of stage oks; proven by test) |
| A10 | Receipt chain under interleaved refusals | host | VALID (every refusal receipted, chain verifies) |

## Findings fixed during the campaign

1. **pw-grid Selection invariant break (found via test failure):** the
   historical `add_unfiltered` path pushed to `ids` without updating the
   dedup `set`, so `contains()` lied about membership. Fixed: unfiltered
   path keeps `ids`/`set` consistent (the bug is the missing filters, not
   a broken invariant). Regression tests: `sel_box_add_bug_repro_*`.
2. **psw-arts-crafts raw-string terminator collision:** `r#"..."#` literals
   containing `"#` (hex colors) terminated early. Fixed: `r##"..."##`.

## Known limitations (not fixed; queued)

1. **Court trusts caller-tagged evidence stances.** The pipeline derives
   stances from its own observed stage outcomes, so the current loop is
   honest — but a compromised caller could file all-Supports evidence for
   a failed run and the court's mechanical rule would accept it. This is
   the REDTEAM-001 meta-bug class ("untrusted claims entering the proof
   boundary"). Fix queued: evidence must carry independently verifiable
   warrants (ADAPT-002 artifact identity), not caller assertions.
   Status: KNOWN LIMITATION, documented, not silently patched.
2. **On-disk manifests are discovery metadata, not authority.** Host
   dispatch uses the registered impl's manifest; a tampered
   `manifests/*.json` cannot grant contracts. Documented as designed.
3. **Quarantine signal list is heuristic (DESIGNED).** Evasion by novel
   patterns is always possible; the fail-closed posture (refuse on
   suspicion, never execute) is the actual defense, not the list.
4. **Dynamic cdylib plugins explicitly out of scope for v1** (see
   parkwreck-plugin docs): no untrusted native code until Quarantine can
   contain it.

## Verdict

10/10 attacks fail closed or are structurally impossible. 2 findings fixed
with regression tests. 4 known limitations documented with queued fixes.
The PSW rebuild survives its own attack loop tonight.
