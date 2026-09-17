---
# browserid-pay-r3m9
title: next_step silently discards goal while any agreement is open
status: todo
type: bug
priority: normal
created_at: 2026-09-17T15:02:07Z
updated_at: 2026-09-17T15:02:07Z
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

- [ ] honour goal alongside open agreements
- [ ] never drop it silently — report deferral with a reason
- [ ] test: a goal for a NEW counterparty while an old agreement is stuck
