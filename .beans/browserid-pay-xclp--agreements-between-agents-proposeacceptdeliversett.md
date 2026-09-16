---
# browserid-pay-xclp
title: 'Agreements between agents: propose/accept/deliver/settle/dispute (design draft)'
status: completed
type: feature
priority: normal
created_at: 2026-09-15T13:13:45Z
updated_at: 2026-09-16T16:17:58Z
---

Second design doc: docs/plans/2026-09-15-agreements-design-draft.md. Contract = (proposal commitment, acceptance) on SBO; contract: scope proves authority to bind; escrow as a release-conditioned pay warrant at the custodian; AI adjudicator paid from an escrowed fee term; discovery instead of hardcoded defaults; centralization ledger.

- [x] Write flows-first draft
- [x] Dan review (2026-09-15; built)

Review 2026-09-15: named (not accountable) parties; proposal on chain before acceptance; payer/payee roles; custodian fee + optional maintenance contribution; own SBO chain with path/governance table; public proposals as a mode; access certs per base spec.

## Summary of Changes

The draft was reviewed and built. Every object and flow in `docs/plans/2026-09-15-agreements-design-draft.md` is implemented across the custodian, chain, indexer, judge and the agreements SDK.

- **Objects and flows** — propose / accept / deliver / settle / dispute in `sdk/agreements/src/agreements.mjs`, exposed as MCP tools in `sdk/agreements-mcp/server.mjs`.
- **Commitment vs. public proposals** — `mode: "public"` carries `terms` inline, otherwise a commitment the acceptance is checked against (`agreements.mjs:219,242,244`).
- **Proposal precedes acceptance** — enforced at the indexer, which matches an acceptance to its proposal by the SDK canonical-payload hash (4e423d5).
- **`contract:` scope proves authority to bind** — `CONTRACT_SCOPE` in the custodian.
- **Escrow as a release-conditioned pay warrant** — `crates/custodian/src/escrow.rs`: lock, settle, freeze, ruling payouts.
- **AI adjudicator paid from an escrowed fee term** — `crates/judge/`, with the dispute response phase (9e9160e) and the evidence-patterns paragraph in the ruling prompt (ce1a68e).
- **Custodian fee + ecosystem contribution** — both are escrow terms charged into the total and paid out on settle (`escrow.rs:340-346,522-523`); the contribution default is published at genesis as `/sys/contribution` (`crates/genesis/src/main.rs:307`).
- **Own SBO chain with path/governance table** — `crates/genesis/`, `crates/chain/`.
- **Reputation** — scorecard from on-chain evidence and reputation-gated grants landed on top (82b9e44, 7920e30, fb52522), per the follow-on reputation draft.
- **Human surface** — the pay.dsp.fyi account page: ledger, agents and budgets, agreements, timeline and scorecards (35d0099, da4a3d1, 72152f5).

Deferred, tracked elsewhere: the *chain-enforced minimum* contribution as a root-policy restriction is `browserid-pay-er8j` — the suggested default and the per-contract term both ship. Cross-custodian escrow and unattended deadline release (`browserid-pay-b1ue`) remain open as the draft listed them.
