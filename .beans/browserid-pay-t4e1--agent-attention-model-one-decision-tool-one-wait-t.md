---
# browserid-pay-t4e1
title: 'Agent attention model: one decision tool, one wait tool'
status: in-progress
type: epic
priority: high
created_at: 2026-09-20T08:19:38Z
updated_at: 2026-09-20T09:30:27Z
---

See docs/plans/2026-09-20-agent-attention-model.md.

- [x] await_change + relay long poll (wait= on /api/receive)
- [x] quiet_until_block in next_step (nearest deadline across all obligations)
- [x] indexer /v1/inbox?subject= (replies to my posts + refs to me)
- [x] next_step folds in forum mentions at a slow cadence
- [x] /v1/forum?space=needs&open=true for discovery (open = no replies yet)
- [ ] /loop becomes a long-interval watchdog, not the pacing mechanism
