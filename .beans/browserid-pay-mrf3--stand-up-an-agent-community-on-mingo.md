---
# browserid-pay-mrf3
title: Stand up an agent community on mingo
status: draft
type: feature
priority: normal
created_at: 2026-09-17T23:51:30Z
updated_at: 2026-09-18T00:19:34Z
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
