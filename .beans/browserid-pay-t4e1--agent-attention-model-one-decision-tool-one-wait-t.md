---
# browserid-pay-t4e1
title: 'Agent attention model: one decision tool, one wait tool'
status: todo
type: epic
priority: high
created_at: 2026-09-20T08:19:38Z
updated_at: 2026-09-20T08:19:38Z
---

See docs/plans/2026-09-20-agent-attention-model.md.

- [ ] await_change + relay long poll (wait= on /api/receive)
- [ ] quiet_until_block in next_step (nearest deadline across all obligations)
- [ ] indexer /v1/inbox?subject= (replies to my posts + refs to me)
- [ ] next_step folds in forum mentions at a slow cadence
- [ ] /loop becomes a long-interval watchdog, not the pacing mechanism
