---
# browserid-pay-bh6u
title: Custodian windows are still wall-clock
status: completed
type: task
priority: high
created_at: 2026-09-18T16:14:51Z
updated_at: 2026-09-18T16:33:11Z
---

The agreement clock moved to block heights (`browserid-pay-e1ou`): the judge's response window (`browserid-pay-dtjt`) and the SDK's obligation deadlines (`browserid-pay-1srw`) are heights now. **The custodian's windows are still wall-clock.**

The one that shows is `replace_adjudicator_from` in the escrow view. The SDK compares it with `nowS()` (`agreements.mjs`, `next_step`), which is **self-consistent today** — both ends are unix seconds — so it works, and converting either end alone would silently compare a height against a timestamp. That is why the SDK site is commented rather than changed.

But it is the same class of problem as `e1ou` if it ever rests on a party-supplied value: nominating a replacement adjudicator is the **only exit from a disputed escrow**, so the window that gates it decides whether money can move at all.

- [ ] audit which custodian windows are contractual (gate an action) vs operational (when a sweep runs). Only the first need heights
- [ ] `replace_adjudicator_from` — is it derived from the custodian's own clock, or from anything a party supplied?
- [ ] the abandonment backstop (`browserid-pay-ov7g`) and the adjudicator ruling window (`browserid-pay-6ai6`) land here too
- [ ] warrant `iat`/`exp`/`window_secs` stay wall-clock — they are credential lifetimes, verified against real time by design
- [ ] rename on conversion, as everywhere else: a height and a timestamp are both integers
- [ ] the custodian needs a chain-head read, as the judge and SDK now have

## Resolution (2026-09-18)

Audited every custodian window. They split cleanly into two groups, and only one group needed anything.

### Converted — because a PARTY supplies the value

`latest_deadline` is derived from the terms, so it is party-supplied and belongs in the same class as `e1ou`. It is now `latest_deadline_block`, sent by the SDK at lock alongside `locked_block` (the height the lock landed at), so durations are blocks-minus-blocks. This also fixed `browserid-pay-usj3` — the field was never sent at all, which silently floored the abandonment backstop at one day and left `max_duration` unenforced.

### Left wall-clock — because the CUSTODIAN supplies both ends

`disputed_at`, `locked_at`, `settled_at` are the custodian's own observations, written from its own clock. `replace_adjudicator_from` is `disputed_at + RULING_WINDOW_SECS`, and `replace_adjudicator` compares it against `db::now()` — **both ends are the custodian's clock, and no party can influence either.**

That is materially different from the bug this came from. `e1ou` was about a value an *interested party* wrote and everything downstream trusted. Here the custodian is already trusted with the money itself; trusting it to know what time it is adds no new exposure. Converting would mean a chain-head read on every dispute and every escrow view, for no gain in what an attacker can do.

The residual concern is operational, not adversarial: if the custodian's clock drifts badly, the window that gates the only exit from a disputed escrow moves with it. Worth monitoring, not worth re-architecting.

`window_secs` / `window_start` (the rolling standing-consent spend window) and warrant `iat`/`exp` also stay wall-clock — credential lifetimes, verified against real time by design.

### The one thing to hold onto

**Do not convert one end of a comparison alone.** The SDK site that reads `replace_adjudicator_from` is commented to say so: converting it there while the custodian still emits a timestamp would silently compare a height against unix seconds, which is exactly how these bugs are born.

## Summary of Changes

See `browserid-pay-usj3` for the code. This bean's outcome is the boundary: party-supplied times became heights, custodian-observed times did not, and the reason is written down at both ends.
