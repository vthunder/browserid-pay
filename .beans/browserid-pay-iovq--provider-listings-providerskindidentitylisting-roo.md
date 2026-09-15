---
# browserid-pay-iovq
title: 'Provider listings: /providers/<kind>/<identity>/listing (root policy matched paths, never ids)'
status: completed
type: bug
priority: normal
created_at: 2026-09-15T19:49:27Z
updated_at: 2026-09-15T20:00:17Z
parent: browserid-pay-h1f5
---

The judge's listing (Path /providers/adjudicators/, ID <identity>) was policy-denied: sbo-core grant patterns match an object's PATH only, so the genesis grant /providers/adjudicators/$user could never match. Fix: listings live in their own collection /providers/<kind>/<identity>/ with ID listing; root policy grants post on /providers/<kind>/$user/**; governed policy update via a new dsp-genesis policy-update subcommand (through the daemon's per-repo submit); judge, SDK discovery, indexer, demo, spec, GENESIS.md updated.

- [x] layout + policy in code and docs
- [x] policy update submitted and served (object hash 56e69095…, via the daemon, accepted 2026-09-15; overlay first, confirmed once the DA batch lands)
- [x] judge listing admitted (block 3886143, creator danmills+judge@sandmill.org) + indexed at index.dsp.fyi
- [x] tests (cargo workspace 20 green, sdk 8, demo-local OK)

## Summary of Changes

Listings moved to /providers/<kind>/<identity>/listing; root policy updated on chain via `dsp-genesis policy-update` (through the daemon; confirmed block 3886121, hash 56e69095…). Judge 6e8d5c4 posts the new layout; its listing landed at block 3886143 and index.dsp.fyi serves it. SDK, indexer, demo, spec, GENESIS.md updated.
