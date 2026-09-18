---
# browserid-pay-dtjt
title: 'Judge: order evidence by block, pass heights, fix the prompt'
status: completed
type: task
priority: high
created_at: 2026-09-18T13:38:58Z
updated_at: 2026-09-18T16:03:04Z
blocked_by:
    - browserid-pay-wl7v
---

Step 2 of the block-height timing plan, and worth doing on its own even if the rest slips — it removes the adjudicator's exposure to a forged timestamp with no format change.

Today `main.rs:260-265` builds evidence as `{id, by, confirmed, object}` — payload verbatim, unsorted, no chain time — while `llm.rs` instructs the model to *"read the evidence in chain order"* and *"weigh WHEN each claim was made, which the chain records"*. Neither is supplied. The only timing in context is the party-written `at`.

- [x] include the block per evidence entry and sort by it
- [x] `Case.respond_by` (`cases.rs:29`) becomes a height; `due()` compares against the head
- [x] the 15s sweep stays wall-clock — it decides when to wake, not what is true
- [x] prompt: state that ordering is by block, that deadlines are heights, and that any time-looking payload field is a party's claim
- [x] leave `opened_at`/`updated_at` — local bookkeeping
- [x] blocked by the ObjectView change

## Summary of Changes

The adjudicator no longer sees a party-chosen timestamp as if it were a record.

**Evidence** (`judge/src/main.rs`) is sorted by `block` and each entry carries it. Unconfirmed writes have no block, so they sort **last** rather than being interleaved on a number they do not have — they were submitted, which is all that can be said.

**The response window is a height.** `Case.respond_by` → `respond_by_block`, `due()` takes the chain head, and the sweep reads `head_block()` — it still *wakes* on a 15s wall clock, but what is *due* is decided by the chain. Config `response_window_secs` → `response_window_blocks`, env `JUDGE_RESPONSE_WINDOW` → `JUDGE_RESPONSE_WINDOW_BLOCKS`.

**Renamed, not reinterpreted, in three places** — the same principle as `deadline_block`, because a height and a unix timestamp are both integers:
- the struct field and the SQL column, so an old row cannot be silently misread
- the env var, so an operator's `JUDGE_RESPONSE_WINDOW=3600` cannot quietly become a 20-hour window
- the API field, so SDK readers break loudly (updated: `agreements.mjs`, `agreements-mcp/server.mjs`)

**A migration default that would have moved money.** `cases.rs` adds the column via `ALTER TABLE ... DEFAULT`. At `0`, every case carried over from the wall-clock schema would be instantly "due" and ruled unanswered — a wrong ruling on a window that never ran. It defaults to `i64::MAX` instead: such a case never becomes due, which is visible and harmless.

**The prompt** now says evidence is ordered by `block`, that `block` is the only time no party chose, that any time-looking payload field is validated by nothing, that a `null` block means not-yet-landed, and that ~20s blocks mean small differences are simultaneous. Deadlines are declared to be heights.

**Tests:** a new `evidence_reaches_the_judge_in_chain_order_with_blocks` puts evidence out of id order plus one pending write, and asserts chain order with pending last and ascending blocks. Prompt assertions guard the new wording. 13 judge tests, 57 workspace, clippy clean.

**Not run:** `demo/run-local.mjs` needs a live daemon. It parses and its assertions were updated, but the end-to-end path is unverified.
