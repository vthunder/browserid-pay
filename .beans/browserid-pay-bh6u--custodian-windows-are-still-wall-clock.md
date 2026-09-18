---
# browserid-pay-bh6u
title: Custodian windows are still wall-clock
status: todo
type: task
priority: high
created_at: 2026-09-18T16:14:51Z
updated_at: 2026-09-18T16:14:51Z
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
