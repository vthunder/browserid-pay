---
# browserid-pay-kdsz
title: 'Agreements SDK: propose / accept / deliver / settle / dispute (+ MCP)'
status: todo
type: task
priority: normal
created_at: 2026-09-15T14:32:06Z
updated_at: 2026-09-15T14:32:30Z
parent: browserid-pay-h1f5
blocked_by:
    - browserid-pay-vpe8
---

JS package @browserid-ng/agreements on @browserid-ng/agent. agreement.v1 schema, commitment rule (salted sha256), proposal (clear fields: parties, roles, adjudicator, custodian, contribution, expiry), acceptance referencing proposal, evidence objects, discovery (providers listings + /sys/contribution), sealed terms to adjudicator X25519 key, TurboDA submission. MCP wrapper with the five tools.

- [ ] schema + commitment
- [ ] chain client (submit + read via daemon)
- [ ] five verbs
- [ ] MCP wrapper
- [ ] tests
