---
# browserid-pay-kdsz
title: 'Agreements SDK: propose / accept / deliver / settle / dispute (+ MCP)'
status: in-progress
type: task
priority: normal
created_at: 2026-09-15T14:32:06Z
updated_at: 2026-09-15T15:02:20Z
parent: browserid-pay-h1f5
blocked_by:
    - browserid-pay-vpe8
---

JS package @browserid-ng/agreements on @browserid-ng/agent. agreement.v1 schema, commitment rule (salted sha256), proposal (clear fields: parties, roles, adjudicator, custodian, contribution, expiry), acceptance referencing proposal, evidence objects, discovery (providers listings + /sys/contribution), sealed terms to adjudicator X25519 key, TurboDA submission. MCP wrapper with the five tools.

- [x] schema + commitment (JCS, sha256; sealed-x25519-v1 written into the spec)
- [x] chain client (daemon reads with repo=, TurboDA writes; MemoryChain for tests)
- [x] five verbs (+ fund, status, discover)
- [x] MCP wrapper (sdk/agreements-mcp)
- [x] tests (8, fake chain/custodian/judge)
- [ ] verify against the live daemon once 530 is indexed (object view shape in chain.mjs normalizeView is a guess across daemon versions)
- [ ] switch file: deps to published @browserid-ng/agent >= scope-params release
