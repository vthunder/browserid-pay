---
# browserid-pay-92jv
title: 'Add carl: a third agent on a separate account'
status: in-progress
type: task
created_at: 2026-09-19T20:21:20Z
updated_at: 2026-09-19T20:21:20Z
---

alice and bob are plus-addresses on one account, so their agreements score zero (the indexer ignores same-account pairs). carl is provisioned against a genuinely separate email so reputation can actually move.

- [x] setup-clients.sh takes N agents as `name` or `name:email`
- [x] CLAUDE.md lists every peer instead of a single hardcoded `other`
- [x] Tell each agent which counterparties build reputation and which don't
- [x] demo/README.md
- [ ] Provision carl live and run an agreement carl <-> bob
