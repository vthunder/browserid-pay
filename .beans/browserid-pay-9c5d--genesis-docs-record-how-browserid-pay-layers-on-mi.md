---
# browserid-pay-9c5d
title: 'Genesis docs: record how browserid-pay layers on mingo'
status: completed
type: task
created_at: 2026-09-18T17:42:21Z
updated_at: 2026-09-18T17:42:21Z
---

Both repos now document the cross-dependency, so a regenesis on either side cannot silently drop the other.

**`mingo/deploy/GENESIS.md`** gained *"⚠ This chain does not only carry mingo — read before any regenesis"*: what browserid-pay keeps on 506, that a mingo regenesis destroys both the data and the policy layer, what must be re-applied, and what mingo provides that must not be dropped.

**`browserid-pay/deploy/GENESIS.md`** gained *"Layering on mingo"*: browserid-pay is now a layer, not a chain. It lists what mingo must provide (`/u/$owner/**`, `/sys/names/*`, DNSSEC fragment, `browserid.me`, admin `govern`), what browserid-pay adds by governed update, the order to do it in, and what does not carry over.

The v1 doc is marked superseded in direction rather than deleted — its policy section is the authoritative expression of what has to be applied, even though `crates/genesis` no longer builds the chain that runs.

**The step most likely to be skipped, called out in both:** verify the deployed daemon has the `sbo-pu34` object-id and payload-field conditions **before** publishing the contribution floor. Without them the policy installs, looks correct, and enforces nothing.

- [ ] if `crates/genesis` is repurposed to emit the governed update, point both docs at the new command
- [ ] mingo's doc names the grants inline; if they change in `crates/genesis`, the copy there goes stale. Worth a pointer-only version once the update tooling exists
