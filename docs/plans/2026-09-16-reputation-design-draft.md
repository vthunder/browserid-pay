# Reputation: a scorecard from on-chain evidence, and grants that use it (design draft)

**Date:** 2026-09-16 · **Status:** review draft · discussion with Dan.
Builds on `docs/specs/evidence-patterns.md` and `agreement-v1.md`.

## The one idea

Everything a score needs is already public and attributed: agreements by
party with their state, evidence objects in chain order (handoff, receipt,
delivery, dispute, response), rulings, and attestations from three kinds
of issuer — counterparties, the custodian (`agreement:settled` /
`agreement:disputed`), and the judge (`agreement:ruled`). The Attestation
spec's rule stands: **scores are off chain and subjective**. An indexer
computes a scorecard and signs its view; a human names, in a warrant, which
indexer they trust and what score they require. No single oracle.

## What the chain lets us count (per identity)

| ingredient | source | sybil-resistance |
|---|---|---|
| agreements settled by acknowledgement, as payee / as payer | agreements + `acknowledgement` evidence, custodian `agreement:settled` | counts only agreements whose two agents have **different grantors** — the grantor is in each party's presentation `bundle` on the proposal/acceptance, so "same controlling account on both sides" is detectable and scores zero |
| disputes raised against me / by me, and how they ended | `dispute` evidence, `ruling` (outcome, split), judge `agreement:ruled` | judge-attested |
| responsiveness | `response` evidence within the judge's window; opened commitments (`matched`) | judge-attested |
| promptness | `delivery` / `receipt` block times vs the obligation deadlines — **only visible when terms were public** or the ruling attestation carries a `late` flag | ruling-attested |
| contested handoffs | `receipt` with `contested` | counterparty-attested |
| tenure and volume | first proposal block; count of distinct counterparties (by grantor) | chain order |
| money | not on chain in commitment mode. Optional: the custodian's `agreement:settled` attestation carries a **value band** (`<10`, `10–100`, `100–1k`, …) — leaks little, lets volume mean something | custodian-attested |

**Sybil, stated honestly (Dan, 2026-09-16).** A grantor is a *controlling
account*, not a person: registering a few domains and minting mailboxes is
cheap, so distinct grantors are not distinct humans and nothing here is
sybil-proof. The grantor check only removes the cheapest self-dealing (one
account on both sides). What makes a sybil ring *expensive* is that every
custodian-attested settlement costs the custodian fee and the contribution,
rulings cost the adjudicator fee, and tenure costs time — the score leans on
those attesters (custodian, judge) and on time, and treats
counterparty-only signals as weak. Wording rule: say "controlling account"
or "grantor", never "human", in code, docs and cards.

## The scorecard (one indexer's view, v1)

Components, each 0–1, with the count of distinct counterparty *accounts* (grantors) `n`:

- **completion** — `(settled_ok + 0.5·ruled_mostly_for + 1) / (total + 2)`
  where `settled_ok` = acknowledged **and custodian-attested**,
  `ruled_mostly_for` = ruling gave ≥80% of the price to this side, `total`
  = agreements with a distinct grantor that reached lock. A Bayesian prior of one win, one loss: a fresh
  identity starts at 0.5, not 1.0.
- **dispute record** — `1 − rulings_against / max(total, 1)`.
- **responsiveness** — responded within the window on every dispute
  against me, and every reveal matched: 1; each miss halves it.
- **promptness** — from the judge's ruling attestations (`late`,
  `partial` flags, decided: meaningful signal, small leak): each late or
  partial finding against me costs 0.1, floor 0.

Headline `score` (0–100) = `100 · completion · dispute_record ·
(0.7 + 0.3·responsiveness) · (0.5 + 0.5·min(1, log2(1+n)/4))`. Multiply by
promptness. Read: a clean record with 15 distinct counterparty accounts
over three months scores in the 90s; one counterparty, one clean
agreement: about 45; a lost ruling early: about 20. Every number on the card is shown with the count behind
it — the score is a summary, the card is the evidence.

Also aggregated to the **grantor** (controlling account) across its
agents, since the grantor is the accountable party (principles §4) — with
the caveat above that an account is not a person.

`GET https://index.dsp.fyi/v1/reputation?subject=<identity>` returns the
card, the inputs with object references (verifiable against the daemon),
the formula version, and a signature by the indexer's identity so a
consumer can hold it to its published view.

## Plugging it back in: a grant that says "anyone good enough"

Today a `contract:agreement` grant names counterparties by identity or
domain. Add one parameter:

```json
{ "scope": "contract:agreement",
  "cap": { "amount": "200.00", "currency": "USD", "window": "P30D" },
  "min_reputation": { "indexer": "https://index.dsp.fyi", "score": 70 } }
```

Card: *"enter agreements up to $200.00 every 30 days with any agent rated
70 or better at index.dsp.fyi"*. The monthly cap already exists (`cap.window`).

Enforcement, like every parameter, is by the audience: the **custodian**
fetches the counterparty's card from the named indexer at lock time and
refuses below the score (it records the card it saw, so the decision is
auditable); the **SDK** checks the same at accept, so a party learns early.
`counterparties` and `min_reputation` compose (both must hold). Stricter-wins:
higher score, and an indexer the human chose. Requires `browserid-core`'s
`ScopeParams` to learn the parameter (deny-unknown-fields) plus the label.

## Deliberately not

- No score on chain, no protocol-level formula; other indexers may differ.
- No reputation for anonymous identities: the subject is always an
  attributed identity, aggregated to its human.
- No decay rules yet beyond tenure; revisit with real volume.

## Decided (2026-09-16)

1. The custodian's `agreement:settled` attestation carries a **value band**
   (`lt10`, `10-100`, `100-1k`, `1k-10k`, `gte10k`), never the amount.
2. `min_reputation` names exactly **one** indexer.
3. The judge's `agreement:ruled` attestation carries `late` and `partial`
   flags (and `share`, the fraction of the price awarded to the subject).
