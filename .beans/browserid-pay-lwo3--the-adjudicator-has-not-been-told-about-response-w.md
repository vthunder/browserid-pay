---
# browserid-pay-lwo3
title: The adjudicator has not been told about response windows
status: todo
type: bug
priority: high
created_at: 2026-09-17T20:37:51Z
updated_at: 2026-09-17T20:37:51Z
---

The ruling prompt (`crates/judge/src/llm.rs`) mentions `handoff`, `receipt`, `delivery`, `dispute` and the response phase. It contains NO reference to `acknowledgement`, `not_received`, `not_met`, `unanswered` or `stalled` — the entire model built in `browserid-pay-ibnl`.

That matters because those states are precisely what now route a case TO the adjudicator. An `unanswered` delivery or a `stalled` arrival has no other exit, so the judge is the destination for the mechanism and does not know the mechanism exists.

**The inference the whole withholding mitigation rests on** (`browserid-pay-oz63`): a recipient who claimed `not_received` PROMPTLY, against a deliverer who never cured, is the signal that the delivery was withheld. Conversely a recipient who said nothing for the whole window, then denies receipt at the hearing, said nothing when it was cheap to speak. The judge cannot weigh either today because it is not told the claims exist or when they were made.

The ordering is already on chain — evidence carries `at` and is read in chain order — so this is a prompt and case-assembly change, not new evidence.

## What the judge needs to know

- [ ] `acknowledgement` is an answer to a delivery: absent `ok`, or `ok: true`, accepts it; `ok: false` refuses, with `reason`
- [ ] `not_received` disputes ARRIVAL and is the deliverer's to cure by delivering again; `not_met` admits arrival and disputes the WORK. They are different complaints and carry different weight
- [ ] silence past the response window is `unanswered` — it settles nothing, and is itself a fact about the recipient worth weighing
- [ ] a `stalled` arrival means the argument ran past its absolute limit with no resolution
- [ ] how to weigh promptness: when a claim was made relative to the delivery it answers, and whether a cure followed it
- [ ] what a commitment-only delivery is worth when the recipient denies receipt — the residual question from `browserid-pay-oz63`, which sealed delivery (`browserid-pay-ifz9`) would make moot but which needs an answer until then

Related: `browserid-pay-6ai6` adds a ruling window, which the judge will also need to respect.
