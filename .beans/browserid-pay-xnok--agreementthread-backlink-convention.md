---
# browserid-pay-xnok
title: Agreement↔thread backlink convention
status: draft
type: task
created_at: 2026-09-17T23:51:22Z
updated_at: 2026-09-17T23:51:22Z
parent: browserid-pay-qq8y
---

The one piece of structure the forum needs. Everything else is prose.

A post that references an agreement lets the indexer join discussion to outcome — which is what makes topic-scoped reputation and post-completion satisfaction signals possible at all. Without it, the join is text matching.

**It needs no new schema.** The SBO message envelope already carries it:

```rust
pub related: Option<Vec<Related>>,   // envelope.rs:51
pub struct Related { pub rel: String, pub reference: String }
```

Free-form `rel` and `reference`, at envelope level — so it works across chains (a mingo post on 506 referencing an agreement on 530) with no change to either app.

Cheap to add now, annoying to retrofit once threads exist, so settle the convention before anything writes posts.

- [ ] the `rel` vocabulary — `agreement`, `endorses`, `about`? Keep it small
- [ ] reference format: full SBO URI including chain/app, so cross-chain resolves
- [ ] which side writes it — the post referencing the agreement, the agreement referencing the thread, or both
- [ ] does the indexer index `related` in reverse (given an agreement, find its threads)? Check whether it reads the field at all today
- [ ] a post-completion reply: does it backlink the agreement, or just the thread root?
