---
# browserid-pay-wl7v
title: Read block into ObjectView and add a head-height read
status: completed
type: task
priority: critical
created_at: 2026-09-18T13:38:58Z
updated_at: 2026-09-18T15:56:01Z
---

Step 1 of the block-height timing plan, and the unblocker for everything else in it.

`ObjectView` (`crates/chain/src/daemon.rs:13`) keeps only `{path, id, creator, value, confirmed}`. The daemon already returns `block` per object (`sbo-daemon/src/main.rs:276`); `view()` at :47 just does not read it. And `GET /v1/state-root` returns `{block, state_root}` — the current head — which nothing reads yet.

Breaks nothing, unblocks the judge and SDK work.

- [x] `ObjectView.block: Option<u64>`, populated in `view()` (Option because pending objects have none)
- [x] head-height read against `/v1/state-root`
- [x] confirm the daemon's `block_number` is the DA chain's block number, not an app-local index
- [x] decide caching and what a client does when the head is unreadable

## Summary of Changes

`crates/chain/src/daemon.rs`:

- `ObjectView` gains `block: Option<u64>`, read from the `block` the daemon already returns. `None` for pending objects — they have not landed, so they have no place in chain order and cannot have met a deadline.
- `ChainReader` gains `head_block()`, reading `GET /v1/state-root`. **Deliberately not defaulted**: a fake or a new implementation must decide what now means rather than inherit a silent zero, and a caller that cannot read the head fails closed instead of guessing a deadline.
- Three tests: a confirmed object carries its block, a pending one does not, and a malformed block reads as absent rather than panicking.

The two test fakes (judge, custodian) now hand out **increasing** block numbers per write, so tests exercise real chain ordering rather than HashMap iteration order — which `browserid-pay-dtjt` will depend on.

**No caching yet.** The plan called for a short cache, but nothing reads `head_block()` in anger; adding one now would be speculative. The comment records the fail-closed contract, and the cache belongs with the first real caller.

Validation: the indexer already had this exact shape in its own reader (`crates/indexer/src/chain.rs`: `Obj.block: u64`, `state_root()`), which confirmed both fields exist before any change was made.

Workspace compiles clean, 53 tests pass, clippy silent.
