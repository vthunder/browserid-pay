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
  "obligations": [ { "by": "a@x", "what": "hand over the ten papers", "deadline": 1759000000, "kind": "input" },
                   { "by": "b@y", "what": "deliver the summaries", "deadline": 1760000000, "kind": "deliverable", "depends_on": 0 } ],
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
the listing lives at `/providers/adjudicators/<identity>/` with ID `listing`
(root policy: `/providers/adjudicators/$user/**`, `$user` = the attributed
email — grant patterns match paths, not ids).

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

## Lock — `ID: lock`, `Content-Schema: agreement.lock.v1` (by the custodian named in the proposal)

```json
{ "v": 1, "id": "agr_…", "locked_at": 1760000000 }
```
No amount. The custodian holds price + fees + contribution against the
payer's account and keeps the sealed terms.

## Evidence — `Path: /agreements/<id>/evidence/`, `ID: <random>` (by either party)

```json
{ "v": 1, "id": "agr_…", "kind": "delivery" | "acknowledgement" | "dispute" | "response",
  "by": "b@y", "commitment": "sha256:…", "note": "… (unverified)",
  "reveals": [ { "evidence_id": "ev_…", "sha256": "sha256:…" } ] }   // response only
```
`delivery`: commitment over the deliverable (or the deliverable inline when
public). `acknowledgement` by the payer releases escrow. `dispute` by either
party opens Flow 3. `response`: the disputed-against party's answer — a note
and the hashes of the delivery commitments it is opening (the contents go to
the adjudicator off chain; the chain records that they were opened).

### Dispute response phase

Filing a dispute opens a case at the adjudicator in state `open` with a
**response window** (`respond_by`, adjudicator-configured; 24h reference).
The other party may answer once within it: `POST <adjudicator>/api/cases/<id>/respond
{ presentation (adjudicator audience), note?, reveals: [{ evidence_id, content }] }`
— the adjudicator checks each reveal's `sha256(content)` (string as-is, JSON
in canonical form) against the commitment on that delivery evidence and
records it as `matched` or not. The adjudicator rules when the response is
in, or when the window passes with none. `GET /api/cases/<id>` shows the
window and whether a response exists; `GET /api/cases?party=<identity>`
lists a party's cases. A commitment opened in a matching reveal is judged
as delivered content; a mismatching reveal counts against the revealer; an
unanswered window counts against the non-responder only where the terms
obliged them. Every evidence object also carries `at` (writer-claimed
unix time; chain order is the truth).

**Typed obligations** (`docs/specs/evidence-patterns.md`): each obligation
MAY carry `kind` — `"input"` (something a party must hand over first, met by
a receipt), `"deliverable"` (default, met by a delivery), `"payment"` (met by
the escrow lock) — and `depends_on` (index of the obligation it needs; its
deadline slides by that one's lateness). Missing `kind` = deliverable,
missing `depends_on` = none.

Two more evidence kinds serve the input handoff pattern:

```json
{ "v": 1, "id": "agr_…", "kind": "handoff", "by": "a@x", "at": 1759000000,
  "commitment": "sha256:…", "manifest": { "count": 10, "items": [ { "name": "…", "size": 1234 } ] },
  "obligation": 0, "note": "… (unverified)" }
{ "v": 1, "id": "agr_…", "kind": "receipt", "by": "b@y", "at": 1759000100,
  "handoff": "<evidence id of the handoff>", "ok": true, "obligation": 0 }
```
`handoff`: the provider's commitment over the inputs it sent off-chain (plus
a manifest for the reader). `receipt`: the receiver's answer — `ok: true`
when the received content hashes to the commitment, `ok: false` with
`reason` when it does not or nothing usable arrived (contested).

## Ruling — `ID: ruling` (by the adjudicator named in the proposal)

```json
{ "v": 1, "id": "agr_…", "outcome": "payee" | "payer" | "split",
  "split": { "payee": "30.00", "payer": "20.00" },   // outcome "split" only; sums to the price
  "fee": "escrow",                                  // who bears the adjudicator fee: "escrow" (default, from the escrowed fee) | "payer" | "payee"
  "reasoning": "… (unverified prose)" }
```
The custodian honors one ruling per contract, only from the named
adjudicator, delivered to it by that adjudicator's own presentation
(audience = the custodian). A ruling is issued only after the dispute
response phase (above): on the counterparty's response, or when the
window lapses.

## Custodian escrow API

| route | who | body |
|---|---|---|
| `POST /api/escrow/lock` | payer's agent | `{ presentation (contract audience), acceptance_bundle (the payee's presentation for the contract: the acceptance's `bundle`, or the proposal's when the proposer is the payee), proposal: <proposal payload>, price, fees, contribution, sealed_terms }` → `{ lock }` — money fields are decimal strings or `{ amount, currency }` (the terms' shape) |
| `POST /api/escrow/<id>/acknowledge` | payer's agent | `{ presentation (contract audience) }` → releases to payee, pays fees + contribution, returns adjudicator fee |
| `POST /api/escrow/<id>/dispute` | either party's agent | `{ presentation (contract audience) }` → freezes; returns `{ sealed_terms }` for the adjudicator |
| `POST /api/escrow/<id>/ruling` | the adjudicator | `{ presentation (custodian audience, identity = named adjudicator), ruling }` → settles per ruling |
| `GET /api/escrow/<id>` | any party | state: `binding` / `locked` / `released` / `disputed` / `ruled` |

**Parties are acting identities; accounts are their grantors.** Every
identity in `parties`/`roles`/`adjudicator` is the agent that presents —
the warrant grantee — and the custodian matches it against the caller's
grantee. Money never touches an agent: the payer's grantor (from the lock
presentation) is debited and refunded, the payee's grantor (from
`acceptance_bundle`) is credited, and the escrow view exposes both as
`payer_account` / `payee_account`. The adjudicator is paid as named.

The `contract:agreement` scope's `cap` bounds `price + fees + contribution`;
`counterparties` (if present) must cover the other party; `max_duration`
(if present) must cover the latest obligation deadline minus now.

## Attestations (Attestation spec layout)

After settlement each party MAY post `/<issuer>/attestations/<subject>/agreement:completed`
(value `{ "contract": "agr_…" }`) and the custodian posts
`agreement:settled`; on a ruling the adjudicator posts `agreement:ruled` on
both parties and the custodian `agreement:disputed`.

## Providers (discovery)

`Path: /providers/custodians/<identity>/` and `/providers/adjudicators/<identity>/`,
`ID: listing`, `Content-Schema: provider.v1`, posted by `<identity>` (its
attributed email — the root policy grants `post` on
`/providers/<kind>/$user/**`; the identity is a path segment because grant
patterns match an object's path, never its id). Readers list the `/providers/<kind>/`
prefix and keep objects whose id is `listing` and whose path segment equals
the payload's `identity`:
```json
{ "v": 1, "identity": "judge@dsp.fyi", "origin": "https://judge.dsp.fyi",
  "price": { "amount": "5.00", "currency": "USD" }, "sealing_key": "<x25519 base64url>",
  "conformance": "browserid-pay-adjudicator-v1" }
```
