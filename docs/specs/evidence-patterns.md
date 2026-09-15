# Evidence patterns

**Status:** build spec, 2026-09-16 (bean browserid-pay-sw4h). Companion to
`agreement-v1.md`. The ledger records only what parties post; these are the
standard shapes they post so a dispute has something to read. Parties adapt
them in the terms; the judge reads them as below.

Rule zero: **a note proves nothing.** A claim ("I sent it", "I delivered
ten") is prose. What counts is a commitment, a receipt, a reveal, or public
content — in chain order.

## 1. Input handoff (something one party must give the other first)

Flow: the provider posts `kind: "handoff"` (commitment over the inputs +
manifest) and sends the inputs off-chain → the receiver posts `kind:
"receipt"` referencing the handoff's evidence id: `ok: true` when the
received content hashes to the commitment, `ok: false` + `reason` when it
does not or nothing usable arrived (contested).

Reading: a handoff with an `ok` receipt is a delivered input, dated by the
receipt. A handoff with **no receipt** inside the obligation's window is the
**receiver's** problem to raise (dispute, or a contested receipt) — silence
after a committed handoff reads as received. **No handoff** by the deadline
is the **provider's** breach, and everything that `depends_on` it is blocked,
not late. A contested receipt freezes the question for the judge: the
provider must show the content that hashes to its commitment.

SDK: `handoff(id, { content | commitment, manifest?, obligation?, note? })`,
`acknowledgeReceipt(id, { handoff_id, content? | contested? })`.

## 2. Commit-and-reveal deliverable

Flow: the performer posts `kind: "delivery"` with a commitment over the
deliverable and sends it off-chain → the payer settles, or disputes → in the
dispute's response phase the performer **reveals** (posts the content that
hashes to the commitment) or the deliverable is public from the start.

Reading: a delivery is proven by an **opened commitment** (revealed content
whose hash matches) or a **public deliverable** — never by the delivery
note. An unopened commitment at ruling time is an undelivered obligation.

## 3. Milestone deadlines (typed obligations)

Each obligation: `by`, `what`, `deadline`, `kind` (`input` | `deliverable` |
`payment`), `depends_on` (index of the obligation it needs). Effective
deadline = own deadline + the dependency's lateness (its met time minus its
deadline, if positive). States: `met`, `pending`, `late`, `blocked` (its
dependency is unmet), `handed_off` (input committed, no receipt yet),
`contested`.

Reading: the party **let down** — the counterparty of a late or contested
obligation — holds the dispute right for it; a party cannot be late on an
obligation that is blocked by the other side. `payment` is met by the escrow
lock, so an unfunded contract is the payer's lateness.

SDK: `obligations(id)` derives all of this from the terms, the evidence and
the clock.

## 4. Public deliverables

`deliver(id, { content, public: true })` puts the content on the ledger in
the clear. Reading: proven at posting time; no reveal needed. Use when
secrecy is not wanted (or the deliverable is meant to be public anyway).

## Adjudication reading (paragraph for the judge's prompt)

> Read the evidence in chain order and by kind, never by what a note claims.
> An obligation of kind `input` is met by a `receipt` with `ok: true` for a
> `handoff` of that obligation; a handoff with no receipt within the window
> counts as received unless the receiver contested it; no handoff at all is
> the provider's breach and blocks every obligation that depends on it (a
> blocked party is not late). A `deliverable` is proven only by an opened
> commitment (revealed content hashing to the delivery's commitment) or by
> public content — a `delivery` whose commitment was never opened is
> undelivered, and a performer who was not given the chance to open it (no
> response phase) must not be ruled against for that alone. `payment` is met
> by the lock. Deadlines slide by a dependency's lateness. Apportion price
> to what was proven delivered; assign the dispute right's cost to the party
> that was late or contested, not to the party that raised it.
