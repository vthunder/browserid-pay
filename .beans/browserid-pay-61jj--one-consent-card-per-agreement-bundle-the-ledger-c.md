---
# browserid-pay-61jj
title: 'One consent card per agreement: bundle the ledger, contract and judge grants into one warrant request'
status: completed
type: feature
priority: normal
created_at: 2026-09-15T22:03:05Z
updated_at: 2026-09-15T22:11:39Z
parent: browserid-pay-h1f5
---

Live run 2026-09-15: bob was asked for several cards at accept (standing ledger-write warrant, the per-contract warrant) and alice one more at dispute (judge sign-in). The filed request already supports up to 8 grants on ONE card; the SDK calls obtainWarrant per audience. Fix: propose/accept compute every grant they will need (ledger-write if missing, the contract warrant, and the adjudicator audience since it is known at accept) and file a single request. One record per audience stays (revocation rows), but one ceremony.

## Summary of Changes

SDK: `obtainWarrants(grants)` files ONE request for everything an agreement needs — the per-contract grant, the standing ledger-write grant (first agreement only) and the adjudicator's audience (scope `dispute`) — at propose and accept; `dispute` asks for nothing. `obtainWarrant` stays as a single-grant shim; a caller-supplied single-grant hook is mapped. MCP server keys its two-phase pending approvals by the whole bundle. Tests/demo assert one card with three grants and zero cards at dispute. No agent-SDK change was needed: `requestWarrants` already takes several grants.
