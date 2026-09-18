---
# browserid-pay-wl7v
title: Read block into ObjectView and add a head-height read
status: todo
type: task
priority: critical
created_at: 2026-09-18T13:38:58Z
updated_at: 2026-09-18T13:38:58Z
---

Step 1 of the block-height timing plan, and the unblocker for everything else in it.

`ObjectView` (`crates/chain/src/daemon.rs:13`) keeps only `{path, id, creator, value, confirmed}`. The daemon already returns `block` per object (`sbo-daemon/src/main.rs:276`); `view()` at :47 just does not read it. And `GET /v1/state-root` returns `{block, state_root}` — the current head — which nothing reads yet.

Breaks nothing, unblocks the judge and SDK work.

- [ ] `ObjectView.block: Option<u64>`, populated in `view()` (Option because pending objects have none)
- [ ] head-height read against `/v1/state-root`
- [ ] confirm the daemon's `block_number` is the DA chain's block number, not an app-local index
- [ ] decide caching and what a client does when the head is unreadable
