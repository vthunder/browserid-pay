---
# browserid-pay-1srw
title: 'SDK: derive obligations from block heights, not payload timestamps'
status: completed
type: task
priority: critical
created_at: 2026-09-18T16:12:59Z
updated_at: 2026-09-18T16:20:08Z
---

The remaining core of `browserid-pay-e1ou`, and the largest piece. The judge no longer trusts the forged field (`browserid-pay-dtjt`), but **`deriveObligations` still computes every met/late/slid/response-window decision from the writer-supplied `at`**.

Plan: `docs/plans/2026-09-18-block-height-timing-plan.md` §B.

Not blocked by the chain move: all data is being wiped, so deadlines written in new units before the move are wiped with everything else.

**Terms format** (the rename decided in `browserid-pay-031y`):
- [ ] `obligations[].deadline` → `deadline_block`
- [ ] proposal `expires` → `expires_block`
- [ ] drop `at` from evidence objects entirely (`agreements.mjs:464`)

**Clock:**
- [ ] `windowSecs(iso)` → `windowBlocks(iso)` via a nominal block-rate constant
- [ ] `nowS()` for contractual purposes → the chain head; keep wall-clock for goal bookkeeping (`:745`, `:759`) and standing-consent warrant windows (`:231-237`), which are credential lifetimes
- [ ] a shared constants module — the nominal rate and window lengths must be **one definition**, or the two sides derive different heights from the same terms

**`deriveObligations` (`:1146-1237`):**
- [ ] `now` becomes a required parameter (a height), not a defaulted `nowS()` — keeps the function pure and synchronous
- [ ] `r.at` (:1155), `lastAnswer.at` (:1191), `anchor.at` (:1193) → the evidence object's block
- [ ] deadline sliding (:1202) — same arithmetic, in blocks
- [ ] `responseDeadline = anchor_at + respondSecs` (:1215) → `anchor_block + respondBlocks`
- [ ] `now > deadline` (:1232), `now > clock` (:1227) → height comparisons
- [ ] **new case:** evidence with no block is unconfirmed and has not landed — exclude it from "met", rather than treating a missing value as zero

**Surfaces:**
- [ ] `next_step` copy (:874, :943-964) — windows described as heights, dates as estimates
- [ ] `accept()` expiry check (:405)
- [ ] the evidence reader must carry the block through (`chain.mjs`)
- [ ] warrant coverage (spec `:172`) — convert `deadline_block` to a date **pessimistically** (blocks can only run slower than nominal), then require the warrant `exp` to cover it with margin

## Summary of Changes

The SDK no longer derives anything from a writer-supplied timestamp. `at` is gone from evidence objects, and `deriveObligations` computes every met/late/slid/window decision from the block an object landed in.

**Clock primitives** (`agreements.mjs`): `NOMINAL_BLOCK_SECS = 20` as the one shared definition, and `windowBlocks(iso)` rounding **up** — rounding up can only lengthen a window, and a window slightly too long costs patience while one slightly too short costs someone their right to answer. `DaemonChain.head()` reads `/v1/state-root` and throws rather than falling back.

**`deriveObligations`** takes a required `head` and throws without it. It used to default to the wall clock, which is how `at` came to decide lateness in the first place; a caller that cannot read the head now fails closed instead of having one invented.

**Renamed, not reinterpreted**, per `browserid-pay-031y`: `deadline` → `deadline_block`, `expires` → `expires_block`, `effective_deadline` → `effective_deadline_block`, `met_at` → `met_at_block`, `response_due` → `response_due_block`. Carried into the MCP tool descriptions, the spec, the demo and the agent-facing `CLAUDE.md`.

**Indexer:** `derive_agreements` takes the head — `rebuild` already had it on `root.block` — so whether an offer lapsed is decided by the chain, not the indexer process clock. The agreements table's `expires` column is renamed; **attestation** `issued_at`/`expires` stay wall-clock, since those are issuer credential lifetimes.

**An unconfirmed write meets nothing.** New case that did not exist: a receipt with no block has not landed, so it cannot meet an obligation. Treating a missing height as 0 would have made it the earliest thing on the chain.

**Tests:** the suite no longer sleeps. `MemoryChain.advance(n)` lets blocks pass, because with heights a sleep proves nothing — which also cut the SDK suite from ~4s to 1.5s and removed a timing race. Two new guards: a payload `at` is inert even when a counterparty supplies one (asserted by deep-equality against an honest run), and an unconfirmed receipt does not meet its obligation.

35 SDK tests, 57 Rust tests, clippy clean.

**Left deliberately wall-clock:** goal bookkeeping, standing-consent warrant windows (credential lifetimes), and `replace_adjudicator_from`, which is computed by the custodian and compared here — converting one end alone would silently compare a height against a timestamp. Commented at the site and tracked in `browserid-pay-bh6u`.

**Not run:** `demo/run-local.mjs` needs a live daemon.
