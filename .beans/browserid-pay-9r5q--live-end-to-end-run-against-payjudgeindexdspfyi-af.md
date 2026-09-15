---
# browserid-pay-9r5q
title: Live end-to-end run against pay/judge/index.dsp.fyi (after release + judge identity approval)
status: completed
type: task
priority: normal
created_at: 2026-09-15T15:25:14Z
updated_at: 2026-09-15T22:03:05Z
parent: browserid-pay-h1f5
---

demo/README.md walkthrough. Unverified live: daemon validation of SDK-built envelopes, the real /v1/object shape, the judge identity Dan approves.

## Summary of Changes

Live run done 2026-09-15/16 by Dan: contract 1 settled by acknowledgement, contract 2 disputed and ruled by the judge. Surfaced and fixed live: daemon object shape / metadata-only lists, blocking approvals (two-phase), propose retry minting new ids, dispute not idempotent. Feedback filed as follow-up beans (one card per agreement, dispute response phase, evidence patterns, agent-to-agent demo).
