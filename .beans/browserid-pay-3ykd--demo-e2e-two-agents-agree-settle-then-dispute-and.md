---
# browserid-pay-3ykd
title: 'Demo + e2e: two agents agree, settle; then dispute and ruling'
status: completed
type: task
priority: normal
created_at: 2026-09-15T14:32:06Z
updated_at: 2026-09-16T16:25:18Z
parent: browserid-pay-h1f5
blocked_by:
    - browserid-pay-y48h
    - browserid-pay-kdsz
    - browserid-pay-6rcb
    - browserid-pay-8nxm
---

Scripted demo with two wallet agents, human consent at browserid.me, settle run and dispute run; Playwright/e2e coverage; README walkthrough.

- [x] demo script (demo/run-local.mjs, make demo-local)
- [x] e2e (local mode is the test: settle + dispute/ruling, asserts balances and chain attribution; passes)
- [x] README (demo/README.md: local mode + prod walkthrough with the human approval steps)
- [x] run the prod walkthrough live once the three services are released and the judge identity is approved

## Summary of Changes

The prod walkthrough ran. Live end-to-end on 2026-09-15/16 (tracked in `browserid-pay-9r5q`): contract 1 settled by acknowledgement, contract 2 disputed and ruled by the judge, with the judge identity approved.

The run surfaced and fixed four things live — daemon object shape / metadata-only lists, blocking approvals (two-phase), propose retry minting new ids, dispute not idempotent — and Dan's feedback became the follow-up beans (one consent card per agreement, dispute response phase, evidence patterns, agent-to-agent relay), all since completed.

All three services are up (pay / index / relay / judge .dsp.fyi all 200 as of 2026-09-16) and the chain carries the resulting objects.
