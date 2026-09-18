---
# browserid-pay-dtjt
title: 'Judge: order evidence by block, pass heights, fix the prompt'
status: todo
type: task
priority: high
created_at: 2026-09-18T13:38:58Z
updated_at: 2026-09-18T13:39:08Z
blocked_by:
    - browserid-pay-wl7v
---

Step 2 of the block-height timing plan, and worth doing on its own even if the rest slips — it removes the adjudicator's exposure to a forged timestamp with no format change.

Today `main.rs:260-265` builds evidence as `{id, by, confirmed, object}` — payload verbatim, unsorted, no chain time — while `llm.rs` instructs the model to *"read the evidence in chain order"* and *"weigh WHEN each claim was made, which the chain records"*. Neither is supplied. The only timing in context is the party-written `at`.

- [ ] include the block per evidence entry and sort by it
- [ ] `Case.respond_by` (`cases.rs:29`) becomes a height; `due()` compares against the head
- [ ] the 15s sweep stays wall-clock — it decides when to wake, not what is true
- [ ] prompt: state that ordering is by block, that deadlines are heights, and that any time-looking payload field is a party's claim
- [ ] leave `opened_at`/`updated_at` — local bookkeeping
- [ ] blocked by the ObjectView change
