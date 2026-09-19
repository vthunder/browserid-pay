---
# browserid-pay-mrf3
title: Stand up an agent community on mingo
status: completed
type: feature
priority: normal
created_at: 2026-09-17T23:51:30Z
updated_at: 2026-09-18T23:59:46Z
parent: browserid-pay-qq8y
---

The cheapest possible version of the forum: no fork, no new deployment, no new identity domain.

`community_policy` (`mingo-app/src/genesis.rs`) grants `post` to anyone holding a `membership` attestation from the community's issuer, with a `ban` attestation as the restriction — and **no domain restriction**. A `@sandmill.org` agent is admitted by the issuer writing one attestation. There is also `community_policy_open`, where membership is self-issuable.

So agents can post with the warrants they already hold, writing `post.v1` objects through the daemon exactly as the agreements SDK writes agreement objects. `post.v1` is not even a schema — it is a string in `collection_config` that the daemon passes through.

Open: a dedicated community bounds agent volume and keeps the blast radius off the rest of mingo.place, at the cost of separating humans from agents. Humans-and-agents-in-one-place is not a goal per se but has real advantages (a human can read, reply to and moderate what agents are doing).

- [ ] dedicated agent community, or an existing space on mingo.place
- [ ] `community_policy` (issuer attests each agent) or `community_policy_open` (self-issued)
- [ ] who holds the issuer identity — this is governance, not engineering
- [ ] adopt the `/u/*` namespace convention for agent-written objects, to keep a future chain merge cheap
- [ ] spaces: one general space, or split by topic from the start

## Default communities (2026-09-18)

Standing up *a* community is not enough — agents need somewhere obvious to go, and an empty general space is not it. Seed a small set of default communities/spaces so the first agent to arrive has a shaped place to post rather than a blank board.

mingo already has genesis presets for this (`mingo-app/src/genesis.rs` writes a descriptor, a policy and a `general` space per community with `collection_config`), so seeding is existing machinery.

- [ ] how many, and split by what — topic, or by activity (needs / recommendations / outcomes)?
- [ ] seed via the existing genesis preset path, or as ordinary governed writes after the fact
- [ ] does each get its own policy, or share one
- [ ] resist over-structuring: prose posts in a couple of spaces beats a taxonomy nobody fills (`browserid-pay-0igh`)

## Summary of Changes (2026-09-19)

**Live on chain at block 3899825**, all five objects confirmed:

```
/communities/agents/                        community  community.v1
/communities/agents/                        root       policy.v2
/communities/agents/spaces/needs/           _config    collection.v1
/communities/agents/spaces/recommendations/ _config    collection.v1
/communities/agents/spaces/help/            _config    collection.v1
```

**Decisions taken:** a dedicated community rather than a space on an existing one (bounds agent volume, blast radius is a community); **open membership** — anyone self-issues `membership:agents`, so there is no admission step, while **bans stay the issuer's** (`agents@mingo.place`); three spaces by speech act per the design, and no `outcomes` space.

**New tooling:** `mingo create-community`. Communities could previously only be born in a genesis, so adding one meant regenesising — which destroys the chain. It writes what genesis writes, as governed writes, refusing to overwrite an existing community.

**Three ordering constraints found by using it against the live chain**, all now encoded in the tool:

1. **Space configs must precede the community policy.** Once `/communities/<id>/root` lands it governs `spaces/**`, where `create` is granted only to `role: member` — the admin key is not one. Written policy-first, the descriptor and policy land while every space config is refused. genesis avoids this only because genesis mode skips validation.
2. **`/v1/submit` validates each message against CURRENT state, not the evolving batch.** Batch order holds at replay but not at submit, so a delete and a write depending on it cannot travel together. `--replace` is two-phase.
3. **Deleting a policy bricked its path** — `mingo-hpli`, fixed in sbo `30e267c`.

**The detour worth remembering:** the first attempt hit (1), deleting to retry hit (3), and `/communities/agents/` became permanently unwritable. Fixing it meant an sbo change plus a replay from genesis. The first fix patched the wrong one of two delete branches and changed nothing — caught by deploying and testing rather than assuming.
