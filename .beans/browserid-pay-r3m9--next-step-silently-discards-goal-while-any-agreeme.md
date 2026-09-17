---
# browserid-pay-r3m9
title: next_step silently discards goal while any agreement is open
status: completed
type: bug
priority: normal
created_at: 2026-09-17T15:02:07Z
updated_at: 2026-09-17T15:16:22Z
---

Reproduced 2026-09-17 against the fake chain, mirroring the live stuck agreement `agr_WvGZP2Rsh190PFjD`:

    alice next_step (no goal):   → handoff  (the old agreement)
    alice next_step (WITH goal): → handoff  (identical — goal discarded)

`nextStep` only consults `goal` when nothing is open:

    if (!open.length) { if (goal?.acceptor) return { action: "propose", … } }

So with any agreement in flight, a loop carrying a goal is told to finish the old work forever and the goal has no effect — not refused, silently dropped, which is the worse failure. The agent can still `propose` directly, so it is only the loop-driven path that is blocked — which is the path we ask people to use.

The rule was deliberate ("finish what is started first") and it is wrong: pending work should not block STARTING new work.

Bob is unaffected — he returns `none` on a blocked agreement and moves on to the next, and a second agreement is picked up correctly.

Fix: honour a goal alongside open work — propose when the goal names a counterparty with no open agreement with them — and when it genuinely should wait, say the goal is deferred rather than dropping it.

Deeper: `goal` is a per-call parameter stored nowhere. The MCP server is local and already persists terms to `AGREEMENTS_TERMS_DIR`, so intent could be a local persisted record instead of something the model restates every tick. That is the better shape and it overlaps `browserid-pay-7bzw` / `browserid-pay-0igh`; this bean is the narrow fix.

- [x] honour goal alongside open agreements
- [x] never drop it silently — report deferral with a reason
- [x] test: a goal for a NEW counterparty while an old agreement is stuck

## Summary of Changes

Fixed by giving intent somewhere to live, rather than patching the branch.

**Goals are now first-class and persisted.** `add_goal` / `list_goals` / `update_goal` / `drop_goal`, stored in `goals.json` beside the terms cache — local to the agent, never sent anywhere. Verified to survive a server restart, which is the point: a loop that forgets what it set out to do cannot run.

**Each goal has its own pipeline**: `sourcing` (nobody to ask) → `ready` (counterparty and terms known) → `running` (an agreement exists) → `done` (its agreement settled). Several run in parallel.

**`next_step` no longer takes `goal` as a parameter at all**, so there is nothing left to drop. It walks agreements in flight first — real money is committed to them — then goals, and reports everything non-actionable in `also_waiting` instead of hiding it.

**Each pipeline stage degrades honestly.** There is no agent directory (`browserid-pay-h4mu`) and no counter-offer protocol (`browserid-pay-7bzw`), so `find_counterparty` and `agree_terms` say to ask the human or message an agent you already know. The pipeline shape is right now; the stages get automated as those land.

**`propose` takes a `goal` id** and attaches the agreement, so the pipeline advances instead of proposing the same thing again next tick — which the tests caught immediately.

Guidance updated in BOTH channels, since the last round showed MCP server instructions alone do not land: the CLAUDE.md template in `demo/setup-clients.sh`, regenerated for the live agents.

Tested (21 SDK tests): the full settle path driven by a registered goal; a goal walking sourcing → ready → propose; and the case from this bean — a stuck, funded agreement no longer blocking a second goal, with the open agreement reported in `also_waiting` rather than dropped.
