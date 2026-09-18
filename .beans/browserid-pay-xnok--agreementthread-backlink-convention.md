---
# browserid-pay-xnok
title: Agreement↔thread backlink convention
status: in-progress
type: task
priority: normal
created_at: 2026-09-17T23:51:22Z
updated_at: 2026-09-18T22:28:12Z
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

## Resolution (2026-09-19)

### Correction: the envelope's `related` header does NOT work

This bean (and the design doc, and the epic) claimed the backlink "needs no new schema" because `sbo-core`'s message envelope already carries `related: Option<Vec<Related>>`. **That was wrong, and I asserted it without checking the read path.**

`Related` is declared in the envelope and included in the canonical header ordering, so it is signed — but it is **not a field of `StoredObject`**, is **not returned by `/v1/object` or `/v1/list`**, and **nothing reads it**. Every construction site in the daemon passes `related: None`. A backlink written there is write-only: provably in the wire, invisible to every reader short of re-fetching raw bytes from DA and re-parsing.

Filed upstream as `sbo-hj98`. Fixing it there is the right long-term answer; it is not a prerequisite for this.

### The convention

**The reference lives in the `post.v1` payload**, which is ours to define and which the daemon passes through untouched:

```json
{
  "body": "…the post…",
  "refs": [ { "rel": "agreement", "ref": "/agreements/agr_NuQ09meJSgk6xoWl/" } ]
}
```

**`rel` vocabulary — two values, and resist adding a third until something needs it:**

| rel | means | `ref` |
|---|---|---|
| `agreement` | this post is about that agreement | the agreement's container path |
| `endorses` | this post endorses that identity | the identity (`name@domain`) |

Threading is NOT in here: mingo's `comment.v1` already carries a parent, so replies need no `rel`.

**Direction: the post references the agreement. Never the reverse.** Two reasons, and the second is the one that matters:

1. An agreement object is written before any thread exists, and `/agreements/**` is `create`-only — append-only, no updates — so it *cannot* be amended to point at a thread later.
2. If an agreement could name a thread, a party could point the reputation join at content of its own choosing. One direction keeps the claim where it can be checked: the post says "I am about that agreement", and whether the poster was a party to it is verifiable.

**Reference format: a plain path on this chain** (`/agreements/<id>/`). The cross-chain concern in the original todo is **moot** — agreements and mingo now share app 506 (`browserid-pay-4v59`). The field is a string, so an absolute SBO URI remains possible later without a format change.

**The reverse index is the indexer's job.** It reads `refs` from `post.v1` payloads and builds agreement → posts. Nothing exists today: the indexer does not read `related` (it cannot) and does not read post payloads at all.

**A post-completion reply backlinks the AGREEMENT, not just the thread root.** A reply that only pointed at its root would be orphaned if the root were moved or deleted — and deletion is a moderator power (`browserid-pay-ctnd`). Referencing the agreement directly keeps each reply independently joinable.

### What must be verified before any of this counts for reputation

The join is only as good as the check on it. Per `browserid-pay-ctnd`'s attack list, the indexer must confirm **the poster was a party to the referenced agreement** before the post influences anything. An unverified `refs` entry is a claim by the poster about themselves.

On one chain this is now checkable at **write time** as well, not only in the indexer — a policy can read the referenced agreement's parties. That was the single qualitative argument for merging chains, and it is now available.

- [x] the `rel` vocabulary — `agreement` and `endorses`, nothing else yet
- [x] reference format — a path on this chain; moot cross-chain concern, single chain now
- [x] which side writes it — the post, one direction only, for the reasons above
- [x] does the indexer index `related` in reverse — it does not, and **cannot**: the header is never served (`sbo-hj98`). The reverse index reads the payload instead
- [x] a post-completion reply backlinks the agreement, not the thread root
- [ ] implement: indexer reads `post.v1` `refs`, builds agreement → posts, filters to parties
- [ ] decide whether to ALSO write the envelope `related` for future-proofing once `sbo-hj98` lands — harmless, costs nothing, and makes the migration free
