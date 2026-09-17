# Handoff: a mingo fork as a forum for agents

**Date:** 2026-09-18 · **Status:** brief for a fresh discussion · **Author:** prior session

The question to explore: **fork mingo into a complete forum where agents discuss,
find each other, and negotiate** — rather than building a bespoke directory or
message board inside browserid-pay.

This document exists so that discussion does not re-derive what is already
settled. It states what exists, what was decided and why, and what is genuinely
open.

---

## 1. Why a forum at all

browserid-pay can execute an agreement end to end — propose, accept, fund,
deliver, settle, dispute, rule — but it cannot help two agents **find each other
or work out terms**. In the live runs a human told alice about bob, and alice
picked $50 out of the air.

Two concrete gaps, both open beans:

- **`browserid-pay-h4mu`** — no directory, no way to broadcast a need. `discover`
  lists custodians and adjudicators only; there is no way to list yourself as an
  agent who does work.
- **`browserid-pay-7bzw`** — `accept(id, terms)` is binary. There is no
  counter-offer, no amendment before signature, and no way to decline. The agents
  never negotiate, and that is the API's fault, not theirs: it gives them one
  move.

A survey of the field (2026-09-17) found **nothing** with a broadcast-a-need
primitive. A2A discovery is provider-advertises-then-client-queries only. That
half is unoccupied.

## 2. What already exists (do not rebuild)

**browserid-pay** (`~/src/browserid-pay`), all live:

| piece | what it does |
|---|---|
| custodian (`pay.dsp.fyi`) | accounts, escrow, standing consent, reputation-gated locks |
| adjudicator (`judge.dsp.fyi`) | rules on disputes with an LLM, reading chain evidence |
| indexer (`index.dsp.fyi`) | agreements by party, attestations by subject, signed scorecards |
| relay (`relay.dsp.fyi`) | `POST /api/send`, `POST /api/receive` — authenticated agent-to-agent messages |
| agreements SDK + MCP | the verbs, goals, `next_step`, standing consent |
| chain | own SBO database, Avail turing **app 530**, genesis at block 3885219 |

**The relay already exists and is barely used.** Agents use it to hand over ids
and terms JSON, never to bargain. Worth understanding why before building a
forum: the reason is `7bzw` (no counter-offer verb), not the transport.

## 3. What mingo is

`~/src/mingo` — a federated forum on the same SBO stack. Live at `mingo.place`,
Avail turing **app 506**, same daemon (`da.sandmill.org`).

```
mingo-app   the forum layer: community.v1 schema, genesis presets, a CLI
mingo-idp   the mingo.place identity provider + serves the SPA same-origin
mingo-web   the browser SPA (app.js) — reads confirmed+pending state, signs writes in-browser
```

**Object model, which is the interesting part:**

- schemas: `community.v1`, `post.v1`, `comment.v1`, `reaction.v1`, plus core
  `identity.v1`, `attestation.v1`, `policy.v2`
- paths: `/communities/<id>/`, `/spaces/<name>/` (channels/subforums),
  `/members/` (membership)
- **a community descriptor carries no logic.** Membership, roles and bans are
  *attestations*; access control is *policy*. A community adds almost no new
  machinery — which is exactly why forking it is cheap.

That last point is the crux: threading, identity, membership and moderation are
already expressed in primitives browserid-pay also uses.

## 4. The question that gates everything — `browserid-pay-0igh`

**How much structure does agent-to-agent trade need?**

Read this bean first. The spectrum:

1. **all prose** — maximum expressiveness, nothing queryable or verifiable
2. **prose + light envelope** — free text plus a few typed fields (who, by when,
   budget band). A forum post with a header, essentially.
3. **fully structured** — `rfq.v1` / `quote.v1` with typed obligations. Machine
   matchable, and brittle.

The asymmetry that probably decides it: **agreements must be structured** because
obligations are typed, deadlines slide, and an adjudicator rules on them — that
structure is load-bearing because money hangs off it. **Discovery and negotiation
are not** binding, so nothing forces structure there.

Working hypothesis: prose (or prose + light envelope) up front, hard structure at
the moment of commitment. The prose→typed-terms conversion is itself a step an
agent could do well, and the counterparty checks it before signing.

A forum is a *human* surface. If agents post prose into it, matching is reading.
A `request.v1` post shape would bridge that — human-readable AND
machine-matchable.

## 5. Decisions already made that constrain this

Do not relitigate these without reason:

- **Timings are protocol constants, not negotiated** (`b3gq`). A window the
  proposer picks is a window an attacker tunes, and there are no chain-enforced
  floors yet.
- **Sybil resistance is deliberately parked.** It is the failure mode of any open
  board, reputation cannot bootstrap new agents, and nobody has solved it. Defer
  until it is a real problem; the current problem is distribution.
- **Reputation is transaction-bound by construction.** The custodian issues
  attestations at settlement, the judge at ruling — a rating without a real
  escrow behind it is not expressible. The A2A community is still *debating*
  this; it is shipped here. It is the credible part of any interop pitch.
- **A claim made during discovery is unbacked.** Reputation attaches at
  settlement, not advertisement. Structure in a listing does not buy trust; see
  `ngr0` (endorsements, borrowing mingo's model).

## 6. The alternative on the table — `browserid-pay-ma3t`

A2A (Google → Linux Foundation) is where interop adoption is. Verified against
its spec:

- Agent Cards at `/.well-known/agent-card.json`, or curated registries queryable
  by skill/tag
- task lifecycle `SUBMITTED → WORKING → COMPLETED | FAILED | CANCELED | REJECTED`
- **no payment, escrow, contracts or binding terms — none**
- **no peer identity establishment or trust negotiation** beyond OAuth2 / API
  keys / mTLS

So A2A is transport and orchestration with a hole exactly the shape of this
project. The open strategic question in that bean: is the win *"A2A agents can
hire a browserid-pay agent"*, or *"any A2A pair can settle through us"*? The
second is much bigger and needs nobody to adopt our client.

**This matters for the forum question**: if Agent Cards are the directory format
the world already queries, a forum should perhaps be the *board* (express a need,
discuss) and not the *directory* (who does X). Those answer different questions
and need not be the same surface.

## 7. Related open beans

| bean | why it touches this |
|---|---|
| `0igh` | the structure question — gates the rest |
| `h4mu` | directory vs board vs thread; mingo named as a board candidate |
| `7bzw` | no counter-offer; what a negotiation would even produce |
| `rlun` | unaddressed / first-N proposals — an unaddressed proposal IS a posting, so this and `h4mu` may be one mechanism |
| `ma3t` | A2A interop |
| `ngr0` | endorsements, explicitly borrowing mingo's model |
| `ifz9` | sealed delivery; needs agents to publish an encryption key, which a directory entry would carry anyway |

## 8. Practical notes

- Both chains run on the same daemon, `da.sandmill.org` (mingo app 506,
  browserid-pay app 530). A fork could be a third app, or share 530.
- The daemon validates attribution at write time: **an object posted by an
  identity was posted by that identity or someone authorized to act as it.** This
  is load-bearing throughout browserid-pay — it is why on-chain objects carry no
  embedded credential (`browserid-pay-3a8p`).
- Policy can enforce payload-field conditions and match on object id since
  `sbo-pu34`, which is how the contribution floor works. The same machinery would
  enforce any posting rules.
- Agent behaviour is steered by `CLAUDE.md` (generated by
  `demo/setup-clients.sh`), not by MCP server `instructions` — the latter proved
  far weaker in practice. Anything a forum expects of agents belongs there.

## 9. What a first discussion should probably settle

- Is the forum the **board** (express a need, negotiate) with the directory
  elsewhere (Agent Cards), or both?
- Post shape: prose, or prose + envelope? What is the minimum that makes a need
  filterable?
- Fork mingo, or add an agent-facing community to the existing mingo?
- Does a negotiation thread produce anything the chain records, or does only the
  resulting proposal land? (`7bzw`: an adjudicator ruling on a dispute cannot see
  what was represented during bargaining.)
- Who reads the board — agents polling, or a matching service?
