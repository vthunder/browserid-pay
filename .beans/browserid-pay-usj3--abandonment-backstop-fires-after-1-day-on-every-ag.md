---
# browserid-pay-usj3
title: Abandonment backstop fires after 1 day on every agreement
status: completed
type: bug
priority: critical
created_at: 2026-09-18T16:29:47Z
updated_at: 2026-09-18T16:32:51Z
blocking:
    - browserid-pay-ov7g
---

**The abandonment backstop fires after 1 day on every agreement, regardless of length, and splits the escrow 50/50.**

`backstop_due` (`crates/custodian/src/escrow.rs:564`) intends a window of **twice the agreement's length**, clamped to [1 day, 30 days]:

```rust
let agreement_secs = e.latest_deadline.map(|d| (d - e.locked_at).max(0)).unwrap_or(0);
let window = (2 * agreement_secs).clamp(BACKSTOP_FLOOR_SECS, BACKSTOP_CEILING_SECS);
```

**But `latest_deadline` is never sent.** It is `Option<i64>` on the lock request (`:238`), and nothing populates it — the SDK's `fund()` body (`agreements.mjs`) carries `presentation, proposal, price, fees, contribution, sealed_terms` and no deadline. No test sends it either.

So `agreement_secs` is always `0`, `window` clamps to the floor of **86,400s**, and `expired_at` falls back to `locked_at` (`:837`). Every escrow becomes eligible for the 50/50 abandonment split **one day after it is locked** — for a seven-day agreement, six days before the work is even due.

Two other things break off the same missing field:

- **`max_duration` is unenforced** (`:431`). A human granting standing consent with "do not tie my money up for more than N days" gets no protection, because the value it is compared against is never present.
- **`browserid-pay-ov7g` rests on a false premise.** That bean records *"`latest_deadline` already reaches the custodian at lock, so the duration is known"*. It does not.

Pre-existing, and unrelated to the block-height work (`browserid-pay-e1ou`) — but the fix lands with it, since the deadline it needs is now a block height and `locked_at` is wall-clock, so the subtraction at `:564` would mix units.

- [ ] SDK `fund()` sends the latest obligation deadline
- [ ] custodian takes it as a block height, and records the block the lock landed in, so the subtraction is blocks-minus-blocks
- [ ] convert that DURATION to seconds by the nominal rate for the backstop window — a safety net measured in days does not need block precision
- [ ] `max_duration`: convert **pessimistically** (blocks can only run slower than nominal, so the real duration can only exceed the estimate) and reject on the inflated figure — that is the direction that protects the payer
- [ ] a test that a long agreement gets a long backstop: nothing currently catches this
- [ ] check whether any live escrow was swept early

## Summary of Changes

Fixed, and the missing field turned out to break three things at once.

**The SDK now sends it.** `fund()` computes the maximum `deadline_block` across the obligations and puts `latest_deadline_block` in the lock body.

**The custodian records the height it locked at** (`locked_block`) as well, so the agreement's length is a blocks-minus-blocks subtraction. Previously `latest_deadline - locked_at` would have mixed a height against a wall-clock time — the fix and the block-height conversion had to land together.

**Three things were broken by the one missing field:**
1. the backstop fell to its 1-day floor on every escrow, however long the agreement
2. `max_duration` on a standing grant was never enforced
3. `browserid-pay-ov7g` was planned on the premise that the deadline already reached the custodian

**`max_duration` converts pessimistically**, per the spec rule: blocks can only run slower than nominal, so the real duration can only exceed the estimate. The estimate is inflated by 25% before comparing, because the grant is the payer's protection and erring the other way would admit an agreement that commits their money longer than they agreed.

**The backstop converts blocks → seconds** rather than moving to heights. A safety net measured in days does not need block precision, and the sweep already runs on a wall clock. Only the *subtraction* needed to be in blocks.

**Why the bug survived:** `backstop_due` was correct, and its unit test passed, while nothing ever populated the field it reads. A unit test on the function could not catch that. The new test goes through `lock` and asserts the deadline and the locked height both survive, then that a ten-day agreement gets a twenty-day backstop.

Shared `chain::NOMINAL_BLOCK_SECS` / `blocks_to_secs`, mirroring the SDK constant so the two cannot drift.

**Not checked:** whether any live escrow on 530 was swept early. That chain is being abandoned (`browserid-pay-4v59`), so it is moot in practice.

58 Rust tests, 35 SDK tests, clippy clean.
