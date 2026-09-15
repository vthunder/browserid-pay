---
# browserid-pay-iovq
title: 'Provider listings: /providers/<kind>/<identity>/listing (root policy matched paths, never ids)'
status: in-progress
type: bug
priority: normal
created_at: 2026-09-15T19:49:27Z
updated_at: 2026-09-15T19:49:27Z
parent: browserid-pay-h1f5
---

The judge's listing (Path /providers/adjudicators/, ID <identity>) was policy-denied: sbo-core grant patterns match an object's PATH only, so the genesis grant /providers/adjudicators/$user could never match. Fix: listings live in their own collection /providers/<kind>/<identity>/ with ID listing; root policy grants post on /providers/<kind>/$user/**; governed policy update via a new dsp-genesis policy-update subcommand (through the daemon's per-repo submit); judge, SDK discovery, indexer, demo, spec, GENESIS.md updated.

- [x] layout + policy in code and docs
- [ ] policy update submitted and served
- [ ] judge listing admitted + indexed
- [ ] tests (cargo workspace, sdk, demo-local)
