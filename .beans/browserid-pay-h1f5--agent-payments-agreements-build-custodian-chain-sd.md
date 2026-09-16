---
# browserid-pay-h1f5
title: Agent payments + agreements build (custodian, chain, SDK, adjudicator)
status: completed
type: epic
priority: normal
created_at: 2026-09-15T14:32:06Z
updated_at: 2026-09-16T16:31:16Z
---

Umbrella for the build of docs/plans/2026-09-11 (payments) and 2026-09-15 (agreements). Order: custodian core → infra → chain genesis → agreements on the internal ledger → adjudicator → demo → x402.

## Summary of Changes

The build this epic ordered — custodian core → infra → chain genesis →
agreements on the internal ledger → adjudicator → demo — is complete, live
and exercised end to end. Only the final step, `→ x402`, is outstanding, and
deliberately so: the system settles in test money, so the rail work carries
on as `browserid-pay-f38b` rather than holding this epic open.

What shipped, against the two drafts (`docs/plans/2026-09-11` payments,
`2026-09-15` agreements):

- **Custodian** (`crates/custodian`) — accounts, bundle verification,
  `pay:<rail>` with per-warrant rolling caps, escrow lock / acknowledge /
  ruling, custodian fee, ecosystem contribution, reputation-gated locks,
  chain writer for lock objects and settlement attestations. 15/15 tests.
- **Chain** — own SBO database on Avail turing app 530, genesis v1 at block
  3885219, regenesis runbook (`deploy/GENESIS.md`), `_sbo.dsp.fyi` live.
- **Agreements SDK + MCP** (`sdk/agreements`, `sdk/agreements-mcp`) — the
  five verbs, commitment and public modes, sealed terms, evidence patterns,
  discovery. 10/10 tests.
- **Adjudicator** (`crates/judge`) — rulings with a dispute response phase
  and evidence-pattern reading.
- **Indexer** (`crates/indexer`) — agreements by party, attestations by
  subject, signed scorecard, live at index.dsp.fyi.
- **Relay** (`crates/relay`) — agent-to-agent messages, rate limited and
  fenced as untrusted, live at relay.dsp.fyi.
- **Dashboard** — the designed pay.dsp.fyi account page: ledger, agents and
  budgets, agreements, timeline and scorecards.
- **Infra** — four dokku apps released from CI by GHCR image
  (`git:from-image`), TLS on all four, host never compiles.
- **Live run** (`browserid-pay-9r5q`, 2026-09-15/16) — one contract settled
  by acknowledgement, one disputed and ruled by the judge, with the
  resulting objects and attestations resolvable through the indexer.

Left open as independent items rather than children of a finished epic:
`f38b` (x402 rail — the seed of the next phase), `er8j` (chain-enforced
minimum contribution, deferred by principle), `b1ue` (escrow deadline
release), `hna1` (exercise the role / value_band attestation shape on
chain). The sbo-daemon bug `2e6j` is unparented — it belongs to sbo/mingo,
and only surfaced here because dsp.fyi became the second repo on that node.
