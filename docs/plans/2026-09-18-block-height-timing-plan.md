# Implementation plan: block heights as the agreement clock

**Date:** 2026-09-18 · **Status:** plan, not yet started
**Fixes:** `browserid-pay-e1ou` (critical) · **Related:** `browserid-pay-b3gq`,
`browserid-pay-x9an`, `sbo-xgyr`

---

## 1. The decision

**Every contractually meaningful time becomes a block height.** Obligation
deadlines, proposal expiry, the delivery response window, the adjudicator's
response window and the abandonment backstop are all expressed and evaluated in
block numbers, not unix seconds.

Wall-clock survives in exactly three places, all of them non-binding:

| stays wall-clock | why |
|---|---|
| warrant `iat` / `exp` / `window_secs` (custodian) | credential lifetimes, verified against real time by design — not agreement terms |
| polling schedules (judge sweep, indexer interval) | when a process wakes up, not what it decides |
| display estimates ("≈ Friday 3pm") | cosmetic; being wrong costs nothing |

### Why

- **Nothing self-asserted stays load-bearing.** Height is not a claim. No `W`,
  no skew tolerance, no HLC, no trust in either party's clock.
- **Both sides derive the same view by construction** — integer comparison of a
  number neither controls. `browserid-pay-b3gq` requires this; today it is a
  hope.
- **The clock that measures you is the clock that lets you act.** Delivering is
  an on-chain write. Under wall-clock deadlines a chain stall makes you late
  when you could not possibly have performed; under height deadlines the
  deadline cannot advance while blocks are not being produced.
- **Substrate skips slots, not numbers.** Block height advances only per
  produced block, and blocks cannot be produced faster than the slot rate. So a
  height-derived window is always **≥** its intended wall-clock duration: a
  party can be given more time than intended, never less.
- **It is available now.** `GET /v1/state-root` returns the head `block`, and
  `/v1/list` already returns `block` per object (`sbo-daemon/src/main.rs:276`).
  This does **not** wait on `sbo-xgyr`.

### What we accept

- **Unconfirmed writes have no height**, so a delivery still in the mempool at
  the deadline height is late. Unambiguous and symmetric, but it means the
  effective deadline for a performer is a block or two earlier than the stated
  one. Agents must be told this.
- **Heights do not translate across chains.** Slightly against the grain — SBO
  objects are otherwise agnostic to the chain underneath. Accepted.
- **20s granularity.** Fine, since `W` already forces windows to be tens of
  minutes at minimum.

### The bug this fixes, stated precisely

`docs/specs/agreement-v1.md:114` already says it:

> Every evidence object also carries `at` (writer-claimed unix time; **chain
> order is the truth**).

The spec is right. **The implementation does the opposite** — `deriveObligations`
treats the writer-claimed `at` as truth and never looks at chain order, and the
adjudicator is handed the same field with no chain time to check it against.
This plan brings the implementation back to what the spec always said.

---

## 2. What changes, by component

### A. Spec — `docs/specs/agreement-v1.md`

- `obligations[].deadline`: unix seconds → **block height** (:23-24)
- proposal `expires`: unix seconds → **block height** (:65)
- evidence objects: **drop `at`** entirely (:114, :127, :130). Chain order was
  always the truth; now it is also the only thing present
- document the nominal block rate constant and that dates are estimates
- document the unconfirmed-write rule
- `:172` — "the warrant must cover the latest obligation deadline minus now"
  is now a **cross-domain check** (a wall-clock credential expiry against a
  height deadline) and needs an explicit conversion rule. See §3.

### B. SDK — `sdk/agreements/src/agreements.mjs`

The bulk of the work.

| line | today | becomes |
|---|---|---|
| `:44` `windowSecs(iso)` | ISO duration → seconds | `windowBlocks(iso)` → blocks, via the nominal rate |
| `:54` `nowS()` | `Date.now()/1000` | `nowBlock()` — reads `/v1/state-root`; **async**, which ripples |
| `:358` | default deadline `nowS() + windowSecs(...)` | head + block count |
| `:386` | `expires: nowS() + ...` | head + block count |
| `:405` | `p.expires < nowS()` | `p.expires < head` |
| `:464` | `at: nowS()` in every evidence object | **removed** |
| `:616` | `deriveObligations(..., now: nowS(), ...)` | `now: head` |
| `:1146-1237` | `deriveObligations` internals | see below |
| `:874`, `:943-964` | `next_step` prose about windows | reworded for heights/estimates |
| `:745`, `:759` | goal `created_at`/`updated_at` | **unchanged** — local bookkeeping |
| `:231-237` | `requestStandingConsent` `window`/`max_duration` | **unchanged** — warrant credential |

`deriveObligations` specifically:

- `r.at` (:1155), `lastAnswer.at` (:1191), `anchor.at` (:1193) → the evidence
  object's **block**, which means evidence entries must carry it (see C)
- deadline sliding `o.deadline + max(0, ds.at - dep.deadline)` (:1202) → the
  same arithmetic in blocks
- `responseDeadline = st.anchor_at + respondSecs` (:1215) → `anchor_block +
  respondBlocks`
- `now > deadline` (:1232) and `now > clock` (:1227) → height comparisons
- **evidence with no block (unconfirmed) must be excluded or treated as
  not-yet-landed** — new case that does not exist today

**`nowS()` → `nowBlock()` is the riskiest edit**: it turns a pure synchronous
function into a network read. Either thread the head height in from the caller
(preferred — keeps `deriveObligations` pure and testable) or make it async and
follow the ripple.

### C. Chain reader — `crates/chain/src/daemon.rs`

- `ObjectView` (:13) gains `block: Option<u64>`; `view()` (:47) reads the
  `block` field the daemon already returns. **One-line-ish, unblocks everything
  downstream.**
- add a head-height read against `GET /v1/state-root`
- `Option` because pending objects have none

### D. Judge — `crates/judge/`

- evidence assembly (`main.rs:260-265`) — include the block and **sort by it**.
  The prompt already claims "read the evidence in chain order"; this is the
  first time that is true
- `Case.respond_by` (`cases.rs:29`) — unix seconds → height
- `cases.due(now)` (`cases.rs:137`) — compare against head height; the sweep
  (`main.rs:388`, every 15s) stays wall-clock but reads the head to compare
- `opened_at` / `updated_at` — **unchanged**, local bookkeeping
- ruling attestation `issued_at` (`main.rs:528`) — issuer claim; leave, but it
  is the same class of unverified field and should not become load-bearing
- **prompt (`llm.rs`)** — say evidence is ordered by block and carries heights;
  state that deadlines are heights; and say plainly that any time-looking field
  in a payload is a party's claim, not a record. Today the prompt invites
  timing reasoning the inputs cannot support

### E. Custodian — `crates/custodian/`

Mostly unaffected: warrant `iat`/`exp`/`window_secs` are credential lifetimes.
The one real interaction is the spec's `:172` rule — a warrant's wall-clock
expiry must cover a height-expressed deadline. §3.

### F. Indexer — `crates/indexer/`

Already the closest to right: `proposal_block`, `acceptance_block`,
`lock_block`, `ruling_block` exist (`index.rs:159`), and attestations carry
`block` (:134).

- `agreements.expires` (:122) becomes a height
- reputation responsiveness (`reputation.rs:246`, "answered in the window")
  evaluates in heights
- attestation `issued_at`/`expires` — issuer claims, unchanged

### G. Agent-facing copy — `demo/setup-clients.sh`

`CLAUDE.md` is how agent behaviour is steered. It must explain that deadlines
are block heights, that a stated date is an estimate, and that an unconfirmed
write has not landed.

### H. New shared constants

Nominal block time (20s) and the derived window constants must be **one
definition consumed by every component**, not a number re-typed per client —
otherwise the two sides derive different heights from the same terms, which is
exactly what `b3gq` forbids.

---

## 3. Open questions to settle before coding

1. **Migration.** Live agreements on 530 carry unix-second deadlines. A height
   and a unix timestamp are both integers, so **nothing will fail loudly** —
   an old deadline read as a height is a number in the distant past, and every
   obligation is instantly late. This needs an explicit version discriminator
   (`v: 2`, or a `deadline_block` field distinct from `deadline`), not a silent
   reinterpretation. **This is the single most dangerous part of the change.**
2. **Warrant coverage across domains** (spec `:172`). A wall-clock `exp` must
   cover a height deadline. Convert the deadline to an estimated date using the
   nominal rate, and require a margin — and decide which way to round, since
   blocks can run slower than nominal and push the real deadline later.
3. **Head-height reads.** Who reads `/v1/state-root`, how often, and what
   happens when it is unreachable? A client that cannot read the head cannot
   evaluate a deadline. Cache with a bound?
4. **`deriveObligations` purity.** Thread the head in as a parameter rather
   than fetching inside — keeps it synchronous and testable. Confirm every
   caller can supply it.
5. **Simultaneous rollout.** Both parties' clients run `deriveObligations` and
   must agree. Old and new cannot interoperate, so this is a coordinated
   cutover or a versioned agreement format — and the demo agents, the judge
   and the indexer all move together.
6. **Does `block` mean the same number everywhere?** Confirm the daemon's
   `block_number` is the DA chain's block number (not an app-local index) and
   that it is identical across apps 506 and 530 on the same Avail network.
7. **Reorgs.** Can an object's `block` change after it is first observed? If
   the DA layer can reorg, a deadline evaluation could flip. Probably settled
   by finality, but confirm rather than assume.

---

## 4. Suggested order

1. **`ObjectView.block` + head read** (C). Unblocks everything, breaks nothing.
2. **Judge: sort evidence by block, pass heights, fix the prompt** (D). This
   alone removes the adjudicator's exposure to the forged field, before any
   format change.
3. **Decide the migration discriminator** (§3.1) — before any deadline is
   written in the new units.
4. **Spec + shared constants** (A, H).
5. **SDK** (B), with `deriveObligations` taking the head as a parameter.
6. **Indexer, agent copy** (F, G).
7. **Drop `at`** last, once nothing reads it.

Step 2 is worth doing on its own even if the rest slips: it is small, and it
stops the adjudicator ruling on a number the disputing party chose.
