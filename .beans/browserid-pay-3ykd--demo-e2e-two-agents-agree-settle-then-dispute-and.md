---
# browserid-pay-3ykd
title: 'Demo + e2e: two agents agree, settle; then dispute and ruling'
status: in-progress
type: task
priority: normal
created_at: 2026-09-15T14:32:06Z
updated_at: 2026-09-15T15:24:16Z
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
- [ ] run the prod walkthrough live once the three services are released and the judge identity is approved
