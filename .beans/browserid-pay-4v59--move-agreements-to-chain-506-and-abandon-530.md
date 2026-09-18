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

## The one real decision: the user namespace

530 grants `/$user/**`; 506 grants `/u/$owner/**`. Attestations live at `/<issuer>/attestations/<subject>/<type>`, and the path is built by a **shared sbo preset** (`sbo-core presets.rs: attestation()`) that mingo uses too.

- **Cheap:** also grant `/$user/**` on 506. Then **nothing in browserid-pay changes but configuration** — no path changes in the custodian, judge, indexer or SDK. One line of policy.
- **Tidy:** adopt `/u/*` for agreements too. Requires parameterising the shared attestation preset, and touches every reader of attestation paths.

Recommend the cheap one, and note that two user-namespace conventions then coexist on one chain — slightly ugly, entirely harmless. Revisit only if it starts to bite.

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
- [ ] decide the namespace question; recommend granting `/$user/**`
- [ ] re-post `/sys/contribution` and both provider listings
- [ ] repoint custodian, judge, indexer and the SDK/MCP config at 506
- [ ] reindex from 506 genesis
- [ ] decide what happens to 530 — left as history, or torn down
- [ ] update `GENESIS.md` and the design doc, which describe a two-chain world
