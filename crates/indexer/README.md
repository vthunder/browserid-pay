# indexer — index.dsp.fyi

A reverse index over the browserid-pay SBO database (`sbo+raw://avail:turing:506/`),
read from the daemon at `da.sandmill.org`. Per the SBO Indexer & Client spec it is
an **acceleration, never an authority**: every response carries the object
references (`path`, `id`, `block`, `object_hash`) plus the daemon URL, so a client
re-fetches `GET <daemon>/v1/object?repo=…&path=…&id=…&proof=1` and verifies for
itself.

## Routes

| route | answer |
|---|---|
| `GET /v1/agreements?party=<identity>` | agreements where the identity is proposer, acceptor, payer, payee, adjudicator or custodian |
| `GET /v1/agreements/<id>` | one agreement |
| `GET /v1/attestations?subject=<identity>[&type=<type>]` | attestations *about* a subject (the Attestation spec stores them under the issuer; this is the reverse index it delegates to indexers) |
| `GET /v1/providers/custodians` · `GET /v1/providers/adjudicators` | provider listings (discovery; `/providers/<kind>/<identity>/listing`) |
| `GET /v1/contribution` | `/sys/contribution` |
| `GET /v1/reputation?subject=<identity>` | **non-normative**: this indexer's subjective counts (distinct issuers per attestation type, agreements by state) |
| `GET /healthz` | liveness |

Every list response: `{ results, references, verify: { daemon, repo, object }, state_root: { block, state_root }, indexed_at, indexer }`.
Results are never newer than `state_root` (it is read first on each poll).

## Agreement state (derived, first match wins)

| state | rule |
|---|---|
| `ruled` | a `ruling` object exists at or after the proposal's block |
| `settled` | an `acknowledgement` evidence object exists, or an `agreement:settled` attestation names the contract |
| `disputed` | a `dispute` evidence object exists |
| `locked` | a `lock` object exists at or after the acceptance |
| `accepted` | an `acceptance` at a block ≥ the proposal's whose `proposal_hash` equals `sha256:<the daemon's object_hash of the proposal>` |
| `expired` | no acceptance and the proposal's `expires` has passed |
| `proposed` | otherwise |

An acceptance with no preceding proposal is ignored (dangling paper). *Who*
wrote a lock or ruling is not checked here — the daemon exposes creators as
key hashes, not identities; the custodian and adjudicator enforce that on their
side (the root policy cannot, see `deploy/GENESIS.md`).

## Reputation: the scorecard (`/v1/reputation?subject=…`)

One indexer's **subjective** view, formula `browserid-pay-reputation-v1`
(docs/plans/2026-09-16-reputation-design-draft.md). The subject is an agent
identity, or a **controlling account** (a warrant grantor) — then its
agents' agreements are pooled. Nothing here is sybil-proof: accounts are
cheap to mint; the grantor only lets us refuse to count an agreement an
account made with itself.

Inputs per agreement (all referenced so a client can re-fetch from the
daemon): the subject's role (payer/payee), the counterparty and its
controlling account (read structurally from the warrant inside each party's
presentation `bundle`), state, outcome, and on a ruling this side's
`share`, `late`, `responded`, `reveals_matched` from the judge's
`agreement:ruled` attestation (falling back to the ruling's split).

An agreement **counts** iff it reached lock and the two controlling
accounts are known and differ. With `total` counted agreements and `n`
distinct counterparty accounts:

| component | formula |
|---|---|
| completion | `(settled_ok + 0.5·ruled_mostly_for + 1) / (total + 2)` — acknowledged settlements, rulings awarding ≥ 0.8 of the price; a one-win-one-loss prior |
| dispute_record | `1 − rulings_against / max(total, 1)` — rulings awarding < 0.5 |
| responsiveness | `0.5 ^ misses` — a miss is a dispute against the subject with no response in the window or a reveal that did not match |
| promptness | `max(0, 1 − 0.1 · late_or_partial_findings)` from the judge's ruling attestations; multiplies the headline |
| tenure | `min(1, blocks_since_first_proposal / 388 800)` (≈ 90 days at ~20 s blocks) |

`score = 100 · completion · dispute_record · (0.7 + 0.3·responsiveness) ·
(0.5 + 0.5·min(1, log2(1+n)/4))`, rounded to 0.1. A fresh identity with one
clean agreement scores ≈ 50; three clean agreements with three different
accounts ≈ 60; a lost ruling early → 0.

The card carries `inputs` (with `counted` / `why_not_counted` per
agreement), `counts`, `constants`, the `state_root` the reads were taken
against, and a `signature`: a compact EdDSA JWS over the canonical JSON
(sorted keys, no whitespace) of the card without `signature`, by the key at
`/v1/reputation/key` (generated into `INDEX_DATA_DIR`, next to the
database). Hold the indexer to its view by re-verifying and recomputing.

## Identity matching

Exact string, with the domain part lowercased (local part byte-exact, as the
browserid spec compares identities). The daemon offers no subject→controller
resolution, so `alice` and `alice@x.test` are distinct subjects here. When the
daemon (or an identity object) resolves names, `index::norm` is the one place
to widen.

## What a fully proof-carrying response would add

The Indexer spec's verifiable query carries State Commitment proofs binding
each result — and, for enumerations, the whole subtree — to `state_root`.
This indexer serves the cheap version: references + the root it indexed
against. Adding proofs means fetching each object with `proof=1` (SBOQ
inclusion proof) and, for `/v1/agreements/<id>`, a subtree proof for
`/agreements/<id>/` so the client can verify completeness. The daemon
already serves `proof=1`; the subtree proof endpoint does not exist yet.

## Config (env)

`INDEX_PORT` (3400), `INDEX_ORIGIN`, `DAEMON` (`https://da.sandmill.org`),
`REPO` (`sbo+raw://avail:turing:506/`), `POLL_SECS` (30), `DATABASE_PATH`.
The whole index is rebuilt from the daemon's list reads on every poll — the
database is small, and the daemon is the source of truth.
