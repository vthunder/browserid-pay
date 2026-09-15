---
# browserid-pay-8nxm
title: 'Consent-card labels for pay: and contract: scopes (browserid-ng)'
status: completed
type: task
priority: normal
created_at: 2026-09-15T14:32:06Z
updated_at: 2026-09-15T15:10:38Z
parent: browserid-pay-h1f5
---

dialog.js scopeVerb + native wallet card: render cap parameters ("up to $20.00 every 30 days", "enter agreements up to $50 with anyone at acme.example"). Wallet-owned label table; parameters are attenuations. Ship in browserid-ng + deploy.

- [ ] dialog.js labels
- [ ] native wallet labels
- [ ] e2e
- [ ] deploy

## Summary of Changes

Done in browserid-ng (bean browserid-ng-h2ze, commits ac6167c..a310de7, pushed and deployed via CI 2026-09-15): core ScopeParams cap/counterparties/max_duration, filed lane carries entries verbatim, wallet-owned scope-labels.js, SDK/wallet/spec/e2e updates.
