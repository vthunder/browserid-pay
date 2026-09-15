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
**named parties** (every agent traces to a browserid identity, and every
delegation names who authorized it — whether an identity is a person, an
organization, or another program the protocol cannot say, principles §5),
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
| **proposal** | on chain, **first** — as a commitment (default) or in the clear (`mode: public`, for tenders) | proposer's agent | commitment (or terms) + the proposer's presentation bundle + offer expiry |
| **acceptance** | on chain, referencing the proposal | acceptor's agent | proposal ref + the acceptor's presentation bundle |
| **escrow lock** | on chain | the escrow custodian | contract ref only — no amount |
| **evidence** | on chain | either party | delivery commitment, acknowledgement, deadline attestation — each referencing the contract |
| **ruling** | on chain | the named adjudicator | contract ref, outcome, fee allocation |
| **completion / dispute attestation** | on chain | counterparties, custodian, adjudicator | reputation input (Attestation spec shape) |

The **contract** is the pair (proposal, acceptance), and it exists **iff the
proposal precedes the acceptance in chain order and the acceptance
references it**. An acceptance with no preceding proposal is dangling paper
every reader ignores; a proposal nobody accepts lapses at its offer
expiry. Its identifier is the proposal commitment — `sha256` over the canonical terms plus a salt the
parties share — so the chain carries the identifier, the parties, the
adjudicator, and the escrow custodian, and never the terms. Terms are
revealed only to the adjudicator, on dispute. (Dan: parties may want
contracts private; this is the answer.)

**Terms** (`agreement.v1`, typed): parties with **roles** (`payer`,
`payee`; either may be the proposer — "I pay X if you do Y" and "I do Y if
you pay X" are the same object), obligations with deadlines, price and
currency, escrow custodian and **its fee**, adjudicator and its fee,
evidence rules (what counts as delivered), expiry of the offer. Human-readable text
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

**Two warrants per agent (build finding, 2026-09-15).** The SBO
Authorization spec requires a chain-write warrant's audience to be the
*bare* database reference (a path in the audience is rejected by
`audience_identifies_db`), so the per-contract warrant can never authorize
the on-chain post itself. Each agent therefore holds (a) one **standing
ledger-write warrant** at `sbo+raw://avail:turing:530/` with `action:post`
and `path:` scopes covering `/agreements/**` and its own namespace, which
attributes every object it posts, and (b) one **`contract:agreement`
warrant per contract** at the contract audience, carried inside the
proposal/acceptance `bundle` and verified by the counterparty and the
custodian. The "one warrant per agent" default from the 2026-09-15
build-plan discussion is superseded; the consent cards are one standing
card plus one per agreement.

## Flow 1 — agree

1. Agents negotiate off chain (MCP, A2A, whatever they already speak).
2. Proposer's agent builds the terms, chooses adjudicator and escrow
   custodian (see discovery, below), **posts the proposal** (commitment or
   clear) with its bundle, and sends the terms to the acceptor off chain.
3. Acceptor's agent verifies the proposer's bundle and cap against the
   price, then posts the acceptance (proposal ref + own bundle). The
   contract now **binds**: id, two parties with proven authority,
   adjudicator, custodian.
4. Whichever party is `payer` presents a `pay:escrow` warrant to the
   custodian naming the contract; the custodian locks price + both fees
   and posts the **escrow lock**. The contract is now **active**; the
   performer waits for the lock, not the acceptance. Escrow is a payment
   warrant with a **release condition** — a new condition type at the
   (already stateful) custodian, not a new component.

## Flow 2 — deliver and settle

1. The performing agent posts a delivery commitment (hash of the
   deliverable, or the deliverable itself when public).
2. The paying agent posts an acknowledgement. The custodian releases the
   price to the payee less its own fee, and returns the adjudicator fee. Both agents post
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

## Fees

Three, all disclosed in the terms before acceptance, all published under
the provider's name (discovery, below):

- **Custodian fee** — for escrow; taken at release.
- **Adjudicator fee** — escrowed; paid only on dispute (above).
- **Ecosystem contribution** — an optional `contribution` **term** in
  `agreement.v1` (recipient identity + amount or percentage): just another
  payee, paid by whichever custodian escrows, at release. The **chain
  publishes a suggested baseline** (`/sys/contribution`: recipient +
  percentage); SDKs read it at discovery and fill the default in, so the
  nudge comes from the chain the parties chose, not from our code. The
  consent card shows the line; either party may zero it or change the
  recipient before accepting. **The chain may also enforce a minimum**
  (Dan, 2026-09-15: running the chain and indexer costs real money): a
  root-policy *restriction* on `/agreements/**` requiring
  `contribution.percentage ≥ N`. Validity in SBO is established by every
  reader on replay under the chain's policy, so a below-minimum proposal
  is disregarded by every conforming validator, not only ours — it is a
  rule of *this database*, never of the protocol, and the escape hatch is
  the correct §2 one: another chain. Two consequences: (1) the Policy spec
  needs a payload-field restriction condition (today only `max_size` and
  `schema` exist) — a small extension; (2) the contribution must be a
  **clear field** of the proposal object, not part of the committed terms,
  since a validator cannot check a hashed field — a percentage of a hidden
  price leaks nothing. The chain cannot make a custodian *pay*; paying
  every payee in the terms is a custodian conformance requirement, and a
  skipped contribution is visible to the recipient (contributions arrive
  attributed), so a non-paying custodian is provably nonconforming.
  Contributions are attributable, so the recipient may issue a *supporter*
  attestation indexers can weigh.
  Our custodian may also route a share of its own fee to the same
  recipient; that needs no protocol support.

## Discovery (custodians and adjudicators)

Nothing is hardcoded in the SDK. A proposer's agent finds custodians and
adjudicators through a built-in discovery step — candidate: SBO objects
under a well-known path, each self-describing (price, rails, conformance
version) and reputation-annotated — and the SDK ships a default *list
source*, not a default party. The counterparty sees the chosen custodian
and adjudicator in the terms and may counter. Choosing them is part of the
agreement.

## The chain

browserid-pay runs its **own SBO database** (own Avail app id, own genesis)
— governance differs from mingo's, and mingo's `deploy/GENESIS.md` is the
runbook to copy, lessons included: back up the sys and domain keys before
anything else; emit `/sys/dnssec/<domain>` at genesis so writes work from
block one; automate the DNSSEC evidence refresh. Paths:

| path | who may write | what |
|---|---|---|
| `/sys/*` | sys key | names, root policy, trust anchors, dnssec evidence (as mingo) |
| `/agreements/<id>/` | create: any attributed identity; `evidence/`: the two parties; `ruling`: the named adjudicator; `lock`: the named custodian | proposal, acceptance, lock, evidence, ruling |
| `/providers/custodians/<name>`, `/providers/adjudicators/<name>` | the named identity | self-describing listings: price, rails, conformance version — the discovery source |
| `/<issuer>/attestations/<subject>/<type>` | the issuer (Attestation spec layout) | completion, dispute, ruling, supporter attestations |
| `/sys/contribution` | sys key | the suggested ecosystem contribution (recipient, percentage); the *minimum* lives in the root policy as a restriction |

**Indexing by identity.** Follows the Attestation spec's decision:
storage is keyed by writer authority (neutral `/agreements/<id>/`, since
two parties write into one agreement; attestations under the issuer), and
the **reverse index by identity is the indexer's job**. The proposal names
parties, adjudicator, and custodian, so party→agreements is built from it;
subjects resolve to controllers, so a handle and the email behind it
aggregate. Reputation scores stay off chain and subjective. We build the
browserid-pay indexer with two verifiable queries (Indexer spec):
*agreements by party* and *attestations by subject*; anyone may run
another and score differently from the same facts.

Governance: the sys key holds the root policy; the root policy grants
creation broadly and restricts by role inside each agreement (party,
adjudicator, custodian are named in the proposal, so policy variables can
bind them).

## What we build

An SDK / MCP toolset with five verbs — `propose`, `accept`, `deliver`,
`settle`, `dispute` — over: the `agreement.v1` schema and commitment rule,
the `contract:` scope, escrow release conditions at the custodian, the
discovery step, a reference AI adjudicator, and the live chain above. The demo: two agents agree
a paid task; run one delivers and settles; run two disputes and the
adjudicator rules.

## Centralization ledger

Every default that could harden into a gate, with its escape hatch:

| default | gate risk | escape hatch |
|---|---|---|
| browserid.me's custodian | the only one counterparties accept | custodian **conformance spec**; anyone runs one; contract names it explicitly |
| our AI adjudicator | the only one anyone trusts | adjudicators are plain identities; price + record on chain; contract names it |
| the chain's minimum contribution | a tax on every agreement here | a rule of this database only, set by its governance and visible in the root policy; any other chain sets none |
| the SDK's discovery list source | de facto registry | a list *source*, swappable; entries are self-describing SBO objects anyone can post |
| custodial escrow | money always passes through a company | on-chain escrow as the decentralized option — on the roadmap, not the start |

## Deferred

- Appeal / multi-adjudicator panels.
- On-chain escrow.
- Sub-delegation of `contract:` authority (bean `0ijs` again).
- Cross-custodian escrow (payer and performer at different custodians).
- Legal enforceability: not claimed; the pitch is self-enforcing where
  money is involved, evidenced everywhere else.

## Settled in review (2026-09-15)

- Proposal precedes acceptance on chain; dangling acceptances are ignored.
- Payment runs either direction via `payer`/`payee` roles.
- Proposals may be public (`mode: public`); commitment is the default.
- The custodian receives a sealed copy of the terms at lock time, readable
  only by the named adjudicator, so a vanished agent cannot strand a
  dispute. (Schema detail to fix before build.)
- Access-cert validity follows the base spec; nothing contract-specific.
- Custodian charges a fee. Ecosystem contribution is a contract term with a
  chain-published default, and the chain reserves a policy-enforced
  minimum (Dan: as a matter of principle; the chain costs money to run).
- Identity indexing is the indexer's job, per the Attestation spec.
