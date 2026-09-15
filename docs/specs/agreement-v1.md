# agreement.v1 — the wire shapes (working spec)

**Status:** build spec, 2026-09-15. Follows
`docs/plans/2026-09-15-agreements-design-draft.md`; where they differ this
file wins for code. Terse on purpose.

All objects are SBO objects on the browserid-pay database
(`sbo+raw://avail:turing:530/`), JSON payloads, posted by the party named
below. `<id>` is `agr_` + 16 base64url chars chosen by the proposer.

## Contract audience

`sbo+raw://avail:turing:530/agreements/<id>` — the audience every party's
`contract:agreement` warrant names. A bundle presented for a contract is
`access_cert~assertion~warrant~config_cert` with assertion `aud` and warrant
`audience` equal to this string.

## Terms (private; canonical JSON, sealed to the adjudicator)

```json
{ "v": 1, "id": "agr_…", "salt": "<32 bytes base64url>",
  "title": "…", "description": "… (unverified prose)",
  "obligations": [ { "by": "b@y", "what": "…", "deadline": 1760000000 } ],
  "price": { "amount": "50.00", "currency": "USD" },
  "fees": { "custodian": "0.50", "adjudicator": "5.00" },
  "evidence": "what counts as delivered, in words",
  "contribution": { "recipient": "fund@dsp.fyi", "percentage": 1.0 } }
```

Canonical JSON = RFC 8785 (JCS). `commitment = "sha256:" + hex(sha256(canonical_terms))`.
Sealed copy — **`sealed-x25519-v1`** (defined by the SDK, 2026-09-15; the
adjudicator's `sealing_key` is a raw X25519 public key, base64url):
`epk` = fresh ephemeral X25519 public key (32 bytes); `shared` = X25519(esk,
rpk); `key` = HKDF-SHA256(ikm = shared, salt = epk‖rpk, info =
`"agreement.v1 seal"`, 32 bytes); `ct` = ChaCha20-Poly1305(key, nonce = 12
zero bytes, aad = epk, plaintext = the canonical terms) ‖ 16-byte tag;
`sealed` = base64url(epk ‖ ct). Single-use key ⇒ fixed nonce is safe;
anonymous sender. Carried off-chain to the custodian at lock time and to
the adjudicator on dispute. Reference: `sdk/agreements/src/seal.mjs` (seal) and `crates/judge/src/seal.rs`
(unseal). The judge's listing also carries `sealing_scheme: "sealed-x25519-v1"`;
the listing id is the adjudicator's full identity string (root policy:
`/providers/adjudicators/$user`, `$user` = the attributed email).

**Two warrants per agent.** The chain (sbo-core `audience_identifies_db`)
only honours a presentation whose warrant audience is the *bare* database
reference (`sbo+raw://avail:turing:530/`), so on-chain writes use a
ledger-write warrant at that audience (scopes `action:post`,
`path:/agreements/**`, `path:/<me>/**`); the `contract:agreement` warrant at
the contract audience is what the proposal/acceptance `bundle` fields carry
and is verified by the counterparty and the custodian, never by the chain.

## Proposal — `Path: /agreements/<id>/`, `ID: proposal`, `Content-Schema: agreement.proposal.v1` (by the proposer)

```json
{ "v": 1, "id": "agr_…", "mode": "commitment",
  "commitment": "sha256:…",              // required in commitment mode
  "terms": { … },                        // only in mode "public" (then no commitment)
  "parties": { "proposer": "a@x", "acceptor": "b@y" },
  "roles":   { "payer": "a@x", "payee": "b@y" },
  "adjudicator": "judge@dsp.fyi",
  "custodian": "https://pay.dsp.fyi",
  "contribution": { "recipient": "fund@dsp.fyi", "percentage": 1.0 },   // clear field (chain may enforce a minimum)
  "expires": 1760000000,                 // offer expiry
  "bundle": "<proposer's presentation for the contract audience>" }
```

## Acceptance — `ID: acceptance` (by the acceptor)

```json
{ "v": 1, "id": "agr_…", "proposal_hash": "sha256:<hex of the proposal payload bytes>",
  "bundle": "<acceptor's presentation for the contract audience>" }
```

A contract exists iff `proposal` precedes `acceptance` in chain order and
`proposal_hash` matches. Binding at acceptance; active at lock.

## Lock — `ID: lock` (by the custodian named in the proposal)

```json
{ "v": 1, "id": "agr_…", "locked_at": 1760000000 }
```
No amount. The custodian holds price + fees + contribution against the
payer's account and keeps the sealed terms.

## Evidence — `Path: /agreements/<id>/evidence/`, `ID: <random>` (by either party)

```json
{ "v": 1, "id": "agr_…", "kind": "delivery" | "acknowledgement" | "dispute",
  "by": "b@y", "commitment": "sha256:…", "note": "… (unverified)" }
```
`delivery`: commitment over the deliverable (or the deliverable inline when
public). `acknowledgement` by the payer releases escrow. `dispute` by either
party opens Flow 3.

## Ruling — `ID: ruling` (by the adjudicator named in the proposal)

```json
{ "v": 1, "id": "agr_…", "outcome": "payee" | "payer" | "split",
  "split": { "payee": "30.00", "payer": "20.00" },   // outcome "split" only; sums to the price
  "fee": "escrow",                                  // who bears the adjudicator fee: "escrow" (default, from the escrowed fee) | "payer" | "payee"
  "reasoning": "… (unverified prose)" }
```
The custodian honors one ruling per contract, only from the named
adjudicator, delivered to it by that adjudicator's own presentation
(audience = the custodian).

## Custodian escrow API

| route | who | body |
|---|---|---|
| `POST /api/escrow/lock` | payer's agent | `{ presentation (contract audience), proposal: <proposal payload>, price, fees, contribution, sealed_terms }` → `{ lock }` |
| `POST /api/escrow/<id>/acknowledge` | payer's agent | `{ presentation (contract audience) }` → releases to payee, pays fees + contribution, returns adjudicator fee |
| `POST /api/escrow/<id>/dispute` | either party's agent | `{ presentation (contract audience) }` → freezes; returns `{ sealed_terms }` for the adjudicator |
| `POST /api/escrow/<id>/ruling` | the adjudicator | `{ presentation (custodian audience, identity = named adjudicator), ruling }` → settles per ruling |
| `GET /api/escrow/<id>` | any party | state: `binding` / `locked` / `released` / `disputed` / `ruled` |

The `contract:agreement` scope's `cap` bounds `price + fees + contribution`;
`counterparties` (if present) must cover the other party; `max_duration`
(if present) must cover the latest obligation deadline minus now.

## Attestations (Attestation spec layout)

After settlement each party MAY post `/<issuer>/attestations/<subject>/agreement:completed`
(value `{ "contract": "agr_…" }`) and the custodian posts
`agreement:settled`; on a ruling the adjudicator posts `agreement:ruled` on
both parties and the custodian `agreement:disputed`.

## Providers (discovery)

`/providers/custodians/<name>` and `/providers/adjudicators/<name>` (by the
identity `<name>@…` that runs them):
```json
{ "v": 1, "identity": "judge@dsp.fyi", "origin": "https://judge.dsp.fyi",
  "price": { "amount": "5.00", "currency": "USD" }, "sealing_key": "<x25519 base64url>",
  "conformance": "browserid-pay-adjudicator-v1" }
```
