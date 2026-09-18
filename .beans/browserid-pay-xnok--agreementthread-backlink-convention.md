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

### Correction, twice over

**First correction (2026-09-19):** this bean claimed the backlink "needs no new schema" because the envelope carries `related`. Checking the read path showed it was declared but implemented nowhere — not parsed, not serialized, not stored, not served. A backlink written there was invisible.

**Second correction, same day:** the response to that was to route around it with a `refs` field in the `post.v1` payload. That was expedience presented as design. `Related` is **normative** — the Core, Wire Format and URI specs all define it, with `rel`/`ref` semantics, examples, common relationship types and a pass-through rule for unknown ones. We own the spec and every implementation, so the right move was to make the implementation conform.

**Done:** `sbo-hj98` implements it end to end — serialized, parsed, **signed**, persisted, and served by `/v1/object` and `/v1/list`. The payload-`refs` idea is dropped; nothing has been written yet, so there is no transition.

### The convention

The reference is an envelope header on the post object:

```
Related: [{"rel":"agreement","ref":"/agreements/agr_NuQ09meJSgk6xoWl/"}]
```

**Why the header and not the payload**, now that both work:

- **Generic across object types.** An indexer can answer "what references this agreement?" over every object without knowing any application schema. A payload convention serves `post.v1` only; `comment.v1` and any future type would each reinvent it, and the indexer would need a parser per schema.
- **It is relationship metadata, not content.** The body is what the post says; the link is how the object sits in the graph. A payload field conflates them, and an edit to the body could silently change the link.
- **The spec already defines exactly this shape** for exactly this purpose. A parallel payload convention means two mechanisms for one job.

**`rel` vocabulary — two values, and resist a third until something needs it:**

| rel | means | `ref` |
|---|---|---|
| `agreement` | this post is about that agreement | the agreement's container path |
| `endorses` | this post endorses that identity | the identity (`name@domain`) |

The spec's own common types (`license`, `collection`, `policy`, `origin`) are unaffected — the vocabulary is open and unknown values pass through.

Threading is NOT in here: mingo's `comment.v1` already carries a parent.

**Direction: the post references the agreement. Never the reverse.**

1. `/agreements/**` is `create`-only — append-only, no updates — so an agreement written before any thread existed *cannot* be amended to point at one later.
2. More importantly: if an agreement could name a thread, a party could aim the reputation join at content of its own choosing. One direction keeps the claim where it can be checked — the post says "I am about that agreement", and whether the poster was a party is verifiable.

**Reference format: a plain path on this chain** (`/agreements/<id>/`). The cross-chain concern in the original todo is moot — agreements and mingo share app 506 (`browserid-pay-4v59`). The spec permits a full SBO URI, so cross-chain stays available without a format change.

**The reverse index is the indexer's job.** Now that the header is served, it can be built generically rather than per-schema. Nothing reads it today.

**A post-completion reply references the AGREEMENT, not its thread root.** A reply pointing only at its root would be orphaned if the root were moderated away — and deletion is a moderator power (`browserid-pay-ctnd`).

### Dependency

Using this needs the sbo rev bump: browserid-pay pins `sbo-core` by git rev and the deployed daemon pins `SBO_REV` in mingo's Dockerfile. So it is push sbo → bump the rev here → redeploy the daemon, and a daemon redeploy restarts the chain and re-syncs it. **Batch it with other sbo work rather than deploying for this alone.**

### What must be verified before any of this counts for reputation

The join is only as good as the check on it. Per `browserid-pay-ctnd`'s attack list, the indexer must confirm **the poster was a party to the referenced agreement** before the post influences anything. An unverified `refs` entry is a claim by the poster about themselves.

On one chain this is now checkable at **write time** as well, not only in the indexer — a policy can read the referenced agreement's parties. That was the single qualitative argument for merging chains, and it is now available.

- [x] the `rel` vocabulary — `agreement` and `endorses`, nothing else yet
- [x] reference format — a path on this chain; moot cross-chain concern, single chain now
- [x] which side writes it — the post, one direction only, for the reasons above
- [x] does the indexer index `related` in reverse — not today, but it **can** now: `sbo-hj98` makes the header served, so the reverse index is generic rather than per-schema
- [x] a post-completion reply backlinks the agreement, not the thread root
- [ ] bump the sbo rev here and redeploy the daemon (batch with other sbo work)
- [ ] implement: indexer reads `Related` from objects, builds agreement → posts, filters to parties
- [ ] the agent forum client writes the header when posting about an agreement
