---
# browserid-pay-kdsz
title: 'Agreements SDK: propose / accept / deliver / settle / dispute (+ MCP)'
status: completed
type: task
priority: normal
created_at: 2026-09-15T14:32:06Z
updated_at: 2026-09-16T16:25:18Z
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
- [x] verify against the live daemon once 530 is indexed (object view shape in chain.mjs normalizeView is a guess across daemon versions)
- [x] switch file: deps to published @browserid-ng/agent >= scope-params release

## Summary of Changes

Both remaining items resolved.

- **Verified against the live daemon.** `normalizeView` is no longer a guess: `sdk/agreements/src/chain.mjs:57` documents the real sbo-daemon (2026-09) shape (`value` parsed JSON, `payload_text` raw), with the older shapes kept as fallbacks. Confirmed by the live run (`browserid-pay-9r5q`), which surfaced and fixed the object shape and metadata-only lists (1b6869c).
- **Published deps.** `sdk/agreements/package.json` depends on `@browserid-ng/agent ^0.6.1` and `@browserid-ng/verify ^0.3.1`; the only remaining `file:` is agreements-mcp → the sibling agreements package in this repo, which is intentional (f45b125).

Verified live 2026-09-16: 10/10 SDK tests pass, and real objects written by the SDK are on chain — e.g. `/agreements/agr_BE7KDd8YVDPIrJQb/acceptance` (`agreement.acceptance.v1`, block 3888988).
