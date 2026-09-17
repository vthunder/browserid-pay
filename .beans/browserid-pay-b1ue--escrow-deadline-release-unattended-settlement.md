---
# browserid-pay-b1ue
title: Escrow deadline release (unattended settlement)
status: scrapped
type: task
priority: normal
created_at: 2026-09-15T15:25:14Z
updated_at: 2026-09-17T20:20:27Z
---

Flow 2 step 3 of the agreements draft: no acknowledgement by the deadline ⇒ performer may dispute; no delivery ⇒ payer may. The custodian has no deadline sweep yet; latest_deadline is only checked against max_duration at lock.

## Reasons for Scrapping

Scrapped 2026-09-17 after working through the scenarios. A deadline release is not a missing feature — as specified it would reintroduce the failure that `browserid-pay-ibnl` and `browserid-pay-oz63` exist to remove.

**The party harmed by inaction always has an exit.** A locked escrow freezes the PAYER's money, and the payer can always dispute. Every scenario resolves without a sweep:

| who stops | what happens |
|---|---|
| payer vanishes after funding | payee disputes → ruling |
| payee vanishes without delivering | obligation goes `late`, payer holds the dispute right → refund |
| payee delivers then vanishes | payer acknowledges or disputes |
| both vanish | locked — see `browserid-pay-ov7g` |
| adjudicator never rules | stuck in `disputed` — see `browserid-pay-6ai6` |

**And no direction is neutral for a timer to fire in.** Auto-release to the payee IS `claim_lapsed`, which we deleted: withhold, wait out an inattentive payer, collect. Auto-refund to the payer is the mirror: take the work, go quiet, get your money back. Deciding which way an ambiguous silence resolves *is* adjudication; a clock cannot do it without picking a side, and picking a side is what creates the gamble.

The two genuine liveness holes are narrower than this bean and are filed separately.
