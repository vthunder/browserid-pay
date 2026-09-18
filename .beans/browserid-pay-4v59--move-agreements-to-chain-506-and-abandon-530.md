---
# browserid-pay-4v59
title: Move agreements to chain 506 and abandon 530
status: todo
type: epic
priority: high
created_at: 2026-09-18T15:07:43Z
updated_at: 2026-09-18T15:07:43Z
---

**Decision (Dan, 2026-09-18): abandon app 530 and run agreements on 506, alongside mingo.** One chain for the forum and for agreements. The agreements currently live on 530 do not need to keep working.

This was deferred in `docs/plans/2026-09-18-agent-forum-design.md` §6 and is now decided. It also collapses the migration question in the block-height timing plan into a **hard cutover with no interop code** (`browserid-pay-031y`).

## What has to move onto 506's root policy

506 already has, in the same shape: `/sys/names/*` create + owner-update, the DNSSEC self-auth fragment, and `admin` (sys key) with `post, transfer, delete, govern` on `/**`. So the sys key can install all of this by **governed update — no regenesis**.

Missing, from `crates/genesis/src/main.rs:314-368`:

**Grants**
- `{ to: "*", can: ["post"], on: "/providers/custodians/$user/**" }`
- `{ to: "*", can: ["post"], on: "/providers/adjudicators/$user/**" }`
- `{ to: "*", can: ["create"], on: "/agreements/**" }`

**Restrictions**
- `{ on: "/providers/**", require: { schema: "provider.v1", max_size: 16384 } }`
- `{ on: "/agreements/**", require: { max_size: 65536 } }`
- the contribution floor: `on: "/agreements/*/"`, `id: "proposal"`, with the `fn`/`amount`/`percentage` field conditions

**Plus** the `/sys/contribution` object and the provider listings for the custodian and adjudicator, re-posted on 506.

## The user namespace: adopt `/u/**` (decided, Dan 2026-09-18)

One convention on the chain, not two. Since all app data is being wiped there are no historical objects to move — this is a path change, not a migration, so do it as part of this epic rather than after.

### What `$user` and `$owner` actually mean

From `sbo-core/src/policy/path.rs:13-15`:

- **`$user`** — the acting signer's canonical identity. Always defined for an attributed write.
- **`$owner`** — the object's declared **`Owner` header** (`owner_ref` on update). *Not path-derived.*

And an undefined variable leaves the literal `$owner` token in the pattern, which matches no real path segment — **the pattern fails closed**.

| | 530 `{to: "*", on: "/$user/**"}` | 506 `{to: "owner", on: "/u/$owner/**"}` |
|---|---|---|
| Owner header | not needed | **required, or the write is rejected** |
| who may write | any signer, under their own canonical id | the actor must be authorized as the declared owner |
| ownership | implicit in the path | explicit in the object |

### The concrete work

**browserid-pay never sets an `Owner` header today.** `sdk/agreements/src/envelope.mjs:39` supports it (`if (m.owner) h.Owner = m.owner`) but `agreements.mjs` never passes one, and the custodian and judge write paths do not either — only `crates/chain/src/identity.rs:197`, for identity claims. Without one, every `/u/$owner/**` write fails closed.

**The blast radius is narrow.** Agreements live at `/agreements/**`, granted `{to: "*", can: ["create"]}` with no `$user`/`$owner` in the pattern — **unaffected by this choice**. The only thing in the user namespace is attestations.

- [ ] set `Owner` on every attestation write (custodian, judge)
- [ ] attestation path `/<issuer>/attestations/…` → `/u/<issuer>/attestations/…`; the path comes from the shared `sbo-core presets.rs: attestation()`, so it needs parameterising — coordinate with mingo, which also uses it
- [ ] update every reader of attestation paths: indexer (`index.rs`), the SDK, the judge's evidence reads
- [ ] drop the `/$user/**` grant from browserid-pay's genesis policy — it is not needed on 506

### Two properties worth keeping deliberately

- **Attestations stay stored under the issuer**, not the subject, so a subject still cannot delete claims about themselves. Preserved by the move; do not lose it.
- **An issuer *can* delete attestations it issued** — mingo's grant is `can: ["*"]` on `/u/$owner/**`. Not a regression (530 granted `post, delete` on `/$user/**`) and arguably correct as revocation, but it means the custodian could retract a scorecard. Deliberate, not inherited. `govern` is excluded from `*` by design, so policy itself is safe.

### Sharing `/u/**` across both apps is a feature

The issuers (`pay.dsp.fyi`, `judge.dsp.fyi`) are not mingo users, so there is little practical overlap. Where there *is* — an agent that both posts on the forum and has reputation — one identity having one namespace is exactly what makes the forum↔reputation link natural. That is what the single chain was for.

## Consequences to accept knowingly

- **Reputation resets to zero.** Every attestation and scorecard on 530 is abandoned. Fine for demo data, but reputation is the part of this project that is credible and shipped, so say it out loud rather than discover it.
- **Blast radius.** Money and a public forum now share a chain; a root-policy mistake touches both. The contribution floor already needed care to install; forum rules now need the same.
- **The contribution floor needs an upgraded daemon** (`sbo-pu34` object-id + payload-field conditions). An older validator **silently ignores** both fields and does not enforce the floor. Same daemon serves both apps, so this should be fine — **verify the deployed version rather than assume.**
- **Checkpointing comes for free.** 506 has a dedicated checkpointer key on `/sys/checkpoints/**`; 530 had none.

## What this unlocks

- **Write-time party checks become possible.** The one qualitative argument for merging: a policy on 506 can now see an agreement's parties, so "only a party may post an outcome reply" (`browserid-pay-ctnd`) stops being indexer-only.
- **The forum↔agreement backlink stops being cross-chain** (`browserid-pay-xnok`) — same chain, same block numbering.
- **One indexer sees discussion, agreements and attestations together**, which is what lets forum activity feed reputation at all.

- [ ] verify the deployed daemon enforces `sbo-pu34` field/id conditions **before** publishing the contribution floor
- [ ] governed root-policy update on 506 adding the grants and restrictions above
- [ ] adopt `/u/**` (decided) — see the namespace section above for the work
- [ ] re-post `/sys/contribution` and both provider listings
- [ ] repoint custodian, judge, indexer and the SDK/MCP config at 506
- [ ] reindex from 506 genesis
- [ ] decide what happens to 530 — left as history, or torn down
- [ ] update `GENESIS.md` and the design doc, which describe a two-chain world
