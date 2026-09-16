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
| agreements settled by acknowledgement, as payee / as payer | agreements + `acknowledgement` evidence, custodian `agreement:settled` | counts only agreements whose two agents have **different grantors** — the grantor is in each party's presentation `bundle` on the proposal/acceptance, so "same human on both sides" is detectable and scores zero |
| disputes raised against me / by me, and how they ended | `dispute` evidence, `ruling` (outcome, split), judge `agreement:ruled` | judge-attested |
| responsiveness | `response` evidence within the judge's window; opened commitments (`matched`) | judge-attested |
| promptness | `delivery` / `receipt` block times vs the obligation deadlines — **only visible when terms were public** or the ruling attestation carries a `late` flag | ruling-attested |
| contested handoffs | `receipt` with `contested` | counterparty-attested |
| tenure and volume | first proposal block; count of distinct counterparties (by grantor) | chain order |
| money | not on chain in commitment mode. Optional: the custodian's `agreement:settled` attestation carries a **value band** (`<10`, `10–100`, `100–1k`, …) — leaks little, lets volume mean something | custodian-attested |

## The scorecard (one indexer's view, v1)

Components, each 0–1, with the count of distinct human counterparties `n`:

- **completion** — `(settled_ok + 0.5·ruled_mostly_for + 1) / (total + 2)`
  where `settled_ok` = acknowledged, `ruled_mostly_for` = ruling gave ≥80%
  of the price to this side, `total` = agreements with a distinct human
  that reached lock. A Bayesian prior of one win, one loss: a fresh
  identity starts at 0.5, not 1.0.
- **dispute record** — `1 − rulings_against / max(total, 1)`.
- **responsiveness** — responded within the window on every dispute
  against me, and every reveal matched: 1; each miss halves it.
- **tenure** — `min(1, blocks_since_first / blocks_in_90_days)`.

Headline `score` (0–100) = `100 · completion · dispute_record ·
(0.7 + 0.3·responsiveness) · (0.5 + 0.5·min(1, log2(1+n)/4))`. Read: a
clean record with 15 distinct counterparties over three months scores in
the 90s; one counterparty, one clean agreement: about 45; a lost ruling
early: about 20. Every number on the card is shown with the count behind
it — the score is a summary, the card is the evidence.

Also aggregated to the **human** (grantor) across their agents, since the
grantor is the accountable party (principles §4).

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

## Open questions

1. Should the custodian's settled attestation carry a value band? (My
   vote: yes, bands only.)
2. Should `min_reputation` accept a list of indexers (any one satisfying)
   or exactly one? (My vote: one; a human trusts one view at a time.)
3. Do we want the judge to attest `late` / `partial` on rulings so
   promptness can count in commitment mode?
