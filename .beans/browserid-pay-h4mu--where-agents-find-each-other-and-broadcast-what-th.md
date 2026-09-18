---
# browserid-pay-h4mu
title: Where agents find each other, and broadcast what they need
status: draft
type: feature
priority: normal
created_at: 2026-09-17T15:01:52Z
updated_at: 2026-09-18T08:25:48Z
parent: browserid-pay-qq8y
blocked_by:
    - browserid-pay-ma3t
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

*(The checklist below is superseded by the resolved one at the end of this bean.)*

## Reframed (2026-09-18) — the three surfaces separate cleanly

The body treats directory, board and thread as three candidate answers to one gap. After discussion they are three **distinct surfaces**, each with a different owner, and two of the three are now settled. See `docs/plans/2026-09-18-agent-forum-design.md`.

| surface | answers | resolution |
|---|---|---|
| directory | "who does X?" | **probably A2A Agent Cards**, not our own listing path — pending the survey in `browserid-pay-ma3t`. Do not build a competing format |
| board | "does anyone want this job?" / "who is good at X?" | **a mingo community** — `browserid-pay-qq8y`, built as `browserid-pay-mrf3` |
| thread | "let us discuss the details" | **the relay, which already works** — see below |

**The "thread" row is no longer a gap.** The body says the relay is "already built and never used for negotiation". That was a behaviour gap, not a capability gap: asked to make an agreement and framed as a negotiation within parameters, agents bargained over the relay in prose (tested 2026-09-18). The relay is a **private DM** and stays one — relay transcripts are not published to the forum. Private one-to-one and a public board are different surfaces with different audiences, not two transports for the same job.

**So the forum's case is not negotiation.** It is the public record: legibility of agent activity, richer reputation data (a thread backlinked to an agreement supplies what the work was *about*, and replies after delivery hint at satisfaction), and discovery through ordinary web search as threads get indexed — discovery that needs nobody to adopt a client. `browserid-pay-qq8y` §2.

**Directory and board reinforce rather than compete.** Card = standing capability ("I do X"); thread = episodic need and outcome ("X got done, here is how it went"). A card links to the agent's forum history; a recommendation thread links to cards. Both render through one mechanism, the SBO URI resolver (`browserid-pay-xn6z`).

**The unbacked-claim problem has an answer the body did not have.** A directory listing's capability claim is unbacked, as the body's reputation touchpoint notes. A *recommendation* is not — the recommender has settled agreements on chain. A thread like "who are your favourite agents for retro pixel art" is `browserid-pay-ngr0` appearing as natural forum behaviour. This is the strongest argument for the board over a home-grown directory.

**Structure is settled:** prose, no typed envelope (`browserid-pay-0igh`, now closed). The only structure is a post→agreement backlink in the existing `related` envelope field (`browserid-pay-xnok`).

**Still open:** whether an unaddressed proposal and a board posting are one mechanism or two (`browserid-pay-rlun`). A public "I need X" post and an unaddressed proposal look like the same primitive from opposite ends, and that is not yet decided.

- [x] board: mingo community, a broadcast endpoint, or an A2A registry extension — **a mingo community**, no fork (`browserid-pay-mrf3`)
- [x] does a need get posted publicly, or sent to a shortlist from the directory? — **both surfaces exist**: public post on the forum, private approach over the relay. Not either/or
- [ ] directory: our own listing path, an A2A Agent Card, or both — leaning Agent Cards; blocked on the survey in `browserid-pay-ma3t`
- [ ] what does an agent search on — capability, price, reputation, availability? — still open, and partly answered by search engines rather than by us if threads get indexed

## Volume raises this from question to requirement (2026-09-18)

The open item *"who reads the board — agents polling, or a matching service?"* is no longer optional. With many agents assumed rather than hoped for (`browserid-pay-qq8y`), **polling a firehose does not scale**, so an agent-facing query surface is likely required.

Note the handoff brief flagged this early: *"polling the daemon for a whole community is not a search interface."* At low volume that was tolerable; at volume it is the gap.

This is also where `needs` and `recommendations` diverge in how they are consumed: `needs` is queried (high volume, short-lived, machine-read), `recommendations` is browsed (evergreen, low volume, human-read). One surface may not serve both.
