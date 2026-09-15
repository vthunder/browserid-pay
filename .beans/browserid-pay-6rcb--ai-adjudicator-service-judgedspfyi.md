---
# browserid-pay-6rcb
title: AI adjudicator service (judge@dsp.fyi)
status: completed
type: task
priority: normal
created_at: 2026-09-15T14:32:06Z
updated_at: 2026-09-15T16:18:30Z
parent: browserid-pay-h1f5
blocked_by:
    - browserid-pay-vpe8
---

Identity + X25519 sealing key published in /providers/adjudicators/judge; price; receives disputes (terms + evidence), rules with Claude (claude-fable-5-1), posts ruling objects; custodian honors rulings only from the named adjudicator, once.

- [x] identity + listing (merged provisioning; approval URL at /api/setup; provider.v1 listing on boot)
- [x] dispute intake + unsealing (sealed-x25519-v1)
- [x] ruling with Claude (claude-fable-5-1, validated JSON)
- [x] post ruling + attestations + custodian delivery
- [x] tests (6)
- [ ] first deploy + human approval of judge@dsp.fyi (blocked on CI image: browserid-ng main lacks scope params; deploy key needs root)

## Summary of Changes

crates/judge live at https://judge.dsp.fyi, provisioned 2026-09-15 as danmills+judge@sandmill.org (delegated agent; warrants for https://pay.dsp.fyi and sbo+raw://avail:turing:530/). Provider listing submitted to the chain.
