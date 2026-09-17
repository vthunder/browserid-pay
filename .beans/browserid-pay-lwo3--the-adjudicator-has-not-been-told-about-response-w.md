---
# browserid-pay-lwo3
title: The adjudicator has not been told about response windows
status: completed
type: bug
priority: high
created_at: 2026-09-17T20:37:51Z
updated_at: 2026-09-17T20:49:40Z
---

The ruling prompt (`crates/judge/src/llm.rs`) mentions `handoff`, `receipt`, `delivery`, `dispute` and the response phase. It contains NO reference to `acknowledgement`, `not_received`, `not_met`, `unanswered` or `stalled` — the entire model built in `browserid-pay-ibnl`.

That matters because those states are precisely what now route a case TO the adjudicator. An `unanswered` delivery or a `stalled` arrival has no other exit, so the judge is the destination for the mechanism and does not know the mechanism exists.

**The inference the whole withholding mitigation rests on** (`browserid-pay-oz63`): a recipient who claimed `not_received` PROMPTLY, against a deliverer who never cured, is the signal that the delivery was withheld. Conversely a recipient who said nothing for the whole window, then denies receipt at the hearing, said nothing when it was cheap to speak. The judge cannot weigh either today because it is not told the claims exist or when they were made.

The ordering is already on chain — evidence carries `at` and is read in chain order — so this is a prompt and case-assembly change, not new evidence.

## What the judge needs to know

- [x] `acknowledgement` is an answer to a delivery: absent `ok`, or `ok: true`, accepts it; `ok: false` refuses, with `reason`
- [x] `not_received` disputes ARRIVAL and is the deliverer's to cure by delivering again; `not_met` admits arrival and disputes the WORK. They are different complaints and carry different weight
- [x] silence past the response window is `unanswered` — it settles nothing, and is itself a fact about the recipient worth weighing
- [x] a `stalled` arrival means the argument ran past its absolute limit with no resolution
- [x] how to weigh promptness: when a claim was made relative to the delivery it answers, and whether a cure followed it
- [x] what a commitment-only delivery is worth when the recipient denies receipt — the residual question from `browserid-pay-oz63`, which sealed delivery (`browserid-pay-ifz9`) would make moot but which needs an answer until then

Related: `browserid-pay-6ai6` adds a ruling window, which the judge will also need to respect.

## Summary of Changes

The ruling prompt (`crates/judge/src/llm.rs`, `READING`) now covers the response-window model. `case.evidence` was already passed to the model unfiltered, so acknowledgement objects with their `ok`, `reason` and `at` were reaching it — what was missing was any instruction on what they mean.

Added:

- an `acknowledgement` is the recipient's answer to a delivery: absent `ok` or `ok: true` accepts; `ok: false` refuses with a `reason`
- the two refusals are different complaints. `not_received` disputes ARRIVAL and is the deliverer's to cure; `not_met` ADMITS arrival and disputes the work — so a party who answered `not_met` cannot later claim it never arrived
- silence past the window settles nothing and is itself a fact about the recipient
- **weigh WHEN each claim was made**, which the chain records: a prompt `not_received` with no later delivery curing it is the mark of work committed to but never handed over; a recipient silent for the whole window who denies receipt only at the hearing was silent when speaking was cheap
- opening a commitment at the hearing proves the content EXISTS; it does not prove it ever reached the other party (Dan, 2026-09-17 — weigh the timing record rather than treating a late reveal as delivery, or as automatic failure)

A test asserts the prompt still covers each of these, and that arrival is explained before quality. It is a weak test of a prompt, but it fails loudly if someone rewrites `READING` and drops the model — which is exactly how this gap appeared.

Still true, and unchanged by this: the ledger looks identical whether the deliverer withheld or the recipient lied. The judge now has the timing record to weigh; `browserid-pay-ifz9` is what would make it decidable.
