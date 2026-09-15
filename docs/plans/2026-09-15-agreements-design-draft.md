# Agreements between agents: propose, accept, deliver, settle, dispute (design draft)

**Date:** 2026-09-15 · **Status:** review draft · bean `browserid-pay-xclp`
Discussion with Dan, 2026-09-15. Builds on the payments draft
(`2026-09-11-agent-payments-design-draft.md`). Nothing below is implemented.

The [browserid principles](https://github.com/vthunder/browserid-ng/blob/main/docs/principles.md)
apply here unchanged: named parties (§1), no gatekeepers (§2), borrowed
authority (§4), one protocol for humans and machines (§5), scaffolding with
an exit (§7). Where this draft adds a default, the "Centralization ledger"
at the end names its escape hatch.

## The one idea

browserid and SBO already supply what agent-contract schemes usually lack:
**accountable parties** (every agent traces to a human or organization),
**bounded authority to bind** (a warrant can cap what an agent may agree
to, and the counterparty can verify that cap at signing time), and an
**ordered record verifiable forever offline** (the notary). What is missing
is everything above the record: a contract shape with two signatures, money
that moves on conditions, and someone whose ruling counts. Money and
reputation are the only things that can be enforced, so that is where the
teeth are; everything else is evidence.

## Objects

| object | where | signed by | contains |
|---|---|---|---|
| **proposal** | off chain (shared between parties); commitment on chain | proposer's agent | typed terms (below) + the proposer's presentation bundle |
| **acceptance** | on chain | acceptor's agent | proposal commitment + the acceptor's presentation bundle |
| **evidence** | on chain | either party | delivery commitment, acknowledgement, deadline attestation — each referencing the contract |
| **ruling** | on chain | the named adjudicator | contract ref, outcome, fee allocation |
| **completion / dispute attestation** | on chain | counterparties, custodian, adjudicator | reputation input (Attestation spec shape) |

The **contract** is the pair (proposal, acceptance). Its identifier is the
proposal commitment — `sha256` over the canonical terms plus a salt the
parties share — so the chain carries the identifier, the parties, the
adjudicator, and the escrow custodian, and never the terms. Terms are
revealed only to the adjudicator, on dispute. (Dan: parties may want
contracts private; this is the answer.)

**Terms** (`agreement.v1`, typed): parties, obligations with deadlines,
price and currency, escrow custodian, adjudicator and its fee, evidence
rules (what counts as delivered), expiry of the offer. Human-readable text
is a field, marked unverified, exactly like `message` on the consent card.

## Authority to bind: the `contract:` scope

Each party's bundle in the proposal / acceptance is a presentation to
**audience = the contract** (its commitment id, named by the proposer
before terms are final — a stable name, not the final hash) under a
`contract:agreement` scope. Parameters are attenuations, enforced by the
*counterparty* and the custodian at signing time:

| parameter | meaning |
|---|---|
| `cap` | `{amount, currency}` — the most this agent may commit per contract |
| `counterparties` | allowlist of identities or `*@domain` |
| `max_duration` | longest obligation window |

A contract therefore proves not "these keys agreed" but "**these agents
were authorized by these people to agree to this much**". The human's
consent card reads: "let agent@… enter agreements up to $50 with anyone at
acme.example". Revocation kills the agent's next signing, not standing
contracts — a signed acceptance is a record, not a live grant.

## Flow 1 — agree

1. Agents negotiate off chain (MCP, A2A, whatever they already speak).
2. Proposer's agent builds the terms, chooses adjudicator and escrow
   custodian (see discovery, below), commits, and sends the proposal with
   its bundle to the acceptor.
3. Acceptor's agent verifies the proposer's bundle and cap against the
   price, then posts the acceptance (commitment + own bundle) to SBO.
4. Proposer's agent posts the commitment object. The contract now exists on
   chain: id, two parties with proven authority, adjudicator, custodian.
5. The payer's agent presents a `pay:escrow` warrant to the custodian
   naming the contract; the custodian locks price + adjudicator fee against
   it. Escrow is a payment warrant with a **release condition** — a new
   condition type at the (already stateful) custodian, not a new component.

## Flow 2 — deliver and settle

1. The performing agent posts a delivery commitment (hash of the
   deliverable, or the deliverable itself when public).
2. The paying agent posts an acknowledgement. The custodian releases the
   price to the performer and returns the adjudicator fee. Both agents post
   completion attestations.
3. No acknowledgement by the deadline ⇒ the performer may open a dispute;
   no delivery by the deadline ⇒ the payer may. Either way, Flow 3.

## Flow 3 — dispute

1. A party posts a dispute object; both reveal the terms and evidence to
   the adjudicator over an authenticated channel (off chain).
2. The adjudicator posts a signed ruling: outcome, and how the escrow and
   fee split. The custodian honors rulings only from the adjudicator named
   in the contract, and only once.
3. Attestations follow: dispute count on both parties, ruling count on the
   adjudicator. Appeal is to the humans behind the agents — attribution
   tells you who they are — and is out of scope for v1.

## The adjudicator

Any identity may be one; an **AI adjudicator** is a first-class option and
the one we run first (Dan). Its rulings are attributed to it, its price is
published under its name as an SBO object, and its record (rulings,
reversals) accrues as attestations. **Fee:** a term in the contract,
escrowed up front with the principal, paid only if a dispute is filed,
otherwise returned. Loser-pays and filing fees are contract options, not
defaults (an AI's mistake should not cost the wronged party).

## Discovery (custodians and adjudicators)

Nothing is hardcoded in the SDK. A proposer's agent finds custodians and
adjudicators through a built-in discovery step — candidate: SBO objects
under a well-known path, each self-describing (price, rails, conformance
version) and reputation-annotated — and the SDK ships a default *list
source*, not a default party. The counterparty sees the chosen custodian
and adjudicator in the terms and may counter. Choosing them is part of the
agreement.

## What we build

An SDK / MCP toolset with five verbs — `propose`, `accept`, `deliver`,
`settle`, `dispute` — over: the `agreement.v1` schema and commitment rule,
the `contract:` scope, escrow release conditions at the custodian, the
discovery step, and a reference AI adjudicator. The demo: two agents agree
a paid task; run one delivers and settles; run two disputes and the
adjudicator rules.

## Centralization ledger

Every default that could harden into a gate, with its escape hatch:

| default | gate risk | escape hatch |
|---|---|---|
| browserid.me's custodian | the only one counterparties accept | custodian **conformance spec**; anyone runs one; contract names it explicitly |
| our AI adjudicator | the only one anyone trusts | adjudicators are plain identities; price + record on chain; contract names it |
| the SDK's discovery list source | de facto registry | a list *source*, swappable; entries are self-describing SBO objects anyone can post |
| custodial escrow | money always passes through a company | on-chain escrow as the decentralized option — on the roadmap, not the start |

## Deferred

- Appeal / multi-adjudicator panels.
- On-chain escrow.
- Sub-delegation of `contract:` authority (bean `0ijs` again).
- Cross-custodian escrow (payer and performer at different custodians).
- Legal enforceability: not claimed; the pitch is self-enforcing where
  money is involved, evidenced everywhere else.

## Open questions for review

1. Should the proposal itself ever be on chain in the clear (public
   tenders), or is commitment-only the sole mode for v1?
2. Who holds the salt and terms if a party's agent disappears? Probably
   the custodian receives a sealed copy at escrow time, readable only by the
   adjudicator — worth deciding before the schema is fixed.
3. Does the acceptor's bundle need a fresh access cert per contract, or
   can the same presentation cover several contracts with one proposer?
