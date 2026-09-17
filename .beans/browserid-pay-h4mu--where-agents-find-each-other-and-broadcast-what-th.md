---
# browserid-pay-h4mu
title: Where agents find each other, and broadcast what they need
status: draft
type: feature
priority: normal
created_at: 2026-09-17T15:01:52Z
updated_at: 2026-09-17T15:01:52Z
---

Today the human tells alice about bob. `discover` lists only custodians and adjudicators (`/providers/<kind>/<identity>/listing`); there is no way to list an agent, and no way to say what you are looking for.

Every system surveyed is provider-advertises-then-client-queries — A2A has no broadcast-a-need primitive at all (verified against the spec, 2026-09-17). So the "post what you need" half is genuinely unoccupied.

**Probably several surfaces, not one** (Dan, 2026-09-17). A directory and a board answer different questions, and a forum with threaded replies is a surprisingly good fit for the second:

| surface | answers | candidate |
|---|---|---|
| directory | "who does X?" | `/providers/agents/<identity>/listing` — same machinery as the existing listings; or an A2A Agent Card, which is the shape the rest of the world already queries |
| board / broadcast | "does anyone want this job?" | a channel of the last N posts; or a mingo community aimed at agents |
| thread | "let us discuss the details" | forum replies, or the relay we already have and never use for negotiation |

**Mingo is a real candidate for the board.** `~/src/mingo` is a federated forum on the same SBO stack — communities, identities, signed posts, threading — already built and deployed. An agent-facing community would give broadcast, threading and identity for free on a chain we run. The catch is that a forum is a human surface; how much structure a post needs is `browserid-pay-0igh`.

**Touchpoints, rather than bundling:**
- *Structure* (`0igh`) — a directory entry needs enough structure to filter on; a board post may not.
- *Reputation* (`ngr0`, `hna1`) — a directory is where a scorecard would actually be consumed, and it is the first place an unbacked capability claim does damage. Reputation-gated grants already exist, so "only agents rated ≥ N may answer" is expressible today.
- *A2A* (`ma3t`) — if Agent Cards are the directory format everyone already queries, serving one may beat building our own directory.
- *Sybil* — the failure mode of any open board, and reputation cannot bootstrap new agents. Deliberately parked until it is a real problem.

- [ ] directory: our own listing path, an A2A Agent Card, or both
- [ ] board: mingo community, a broadcast endpoint, or an A2A registry extension
- [ ] does a need get posted publicly, or sent to a shortlist from the directory?
- [ ] what does an agent search on — capability, price, reputation, availability?
