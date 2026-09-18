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
   obligation is instantly late. **This is the single most dangerous part of the
   change.**

   **Decided (Dan, 2026-09-18): rename the field rather than version it.** `deadline_block`
   and `expires_block`, distinct from `deadline` and `expires`. A version
   integer only protects readers that check it; a renamed field fails loudly in
   **both** directions — an old client reading a new agreement finds no
   `deadline` and hits the existing "everything needs a bound" error, and a new
   client seeing `deadline` knows immediately it is looking at the old units.
   No silent misreading is possible.
2. **Warrant coverage across domains** (spec `:172`). A wall-clock `exp` must
   cover a height deadline.

   **Decided (Dan, 2026-09-18): convert pessimistically.** Blocks can only run *slower*
   than nominal, never faster, so the real-world deadline can only land later
   than the estimate. Convert `deadline_block` to a date using the nominal rate
   **plus a margin**, and require `exp` to cover that. Erring the other way
   produces a warrant that expires before the deadline it was meant to cover.
3. **Head-height reads.** Who reads `/v1/state-root`, how often, and what
   happens when it is unreachable? A client that cannot read the head cannot
   evaluate a deadline. Cache with a bound?
4. **`deriveObligations` purity.** Thread the head in as a parameter rather
   than fetching inside — keeps it synchronous and testable. Confirm every
   caller can supply it.
5. **Simultaneous rollout.** Both parties' clients run `deriveObligations` and
   must agree. Old and new cannot interoperate, so this is a coordinated cutover
   — the demo agents, the judge and the indexer all move together.

   **Settled (Dan, 2026-09-18): hard cutover, no interop code.** The agreements
   on 530 do not need to keep working — in fact 530 is being abandoned entirely
   in favour of running agreements on 506 alongside mingo (`browserid-pay-4v59`).
   So there is no dual evaluation path to build: the new units are the only
   units, and the renamed fields exist purely as a guard against a stale client.
6. ~~**Does `block` mean the same number everywhere?**~~ **Settled: yes, it is
   the Avail block number, global across apps.** The sync loop iterates Avail
   block numbers (`sync.rs:1316`, `process_block(block_num)`) and reads
   **several app_ids out of one block** —
   `rpc.fetch_block_data_multi(block_number, &app_ids)` (`sync.rs:561`). Apps
   506 and 530 are therefore numbered by the same chain, and a height means the
   same instant in both.
7. ~~**Reorgs.**~~ **Settled: the daemon only ever processes finalized blocks.**
   `lc.subscribe_blocks()` (`lc.rs:106`, *"Subscribe to new finalized blocks"*)
   reads the avail-light `/v2/status` head, which `main.rs:1599` calls the
   "finalized head". The `rpc_only` fallback still bounds processing by that
   same finalized head — it skips the light-client **availability** sampling,
   not finality. So an object receives a `block_number` only once its block is
   final, and a deadline evaluation cannot flip under it.

   *Caveat, unrelated to timing:* `rpc_only` means trusting a full node's RPC
   for block **data** (no DAS). That is a data-availability posture question for
   production, not a reorg risk.
8. ~~**`deriveObligations` purity.**~~ **Settled: thread the head in as a
   parameter.** It already takes `now` with a default of `nowS()`; make it
   required. The function stays synchronous and testable, and the caller — which
   is already reading chain state for evidence — supplies it.
9. ~~**Head-height reads.**~~ **Settled in shape:** read `GET /v1/state-root`,
   cache for a few seconds (a block is ~20s, so a short cache costs nothing),
   and **fail closed** — a client that cannot read the head must not guess a
   deadline. This adds no new network dependency: every caller is already
   reading the chain for evidence.

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
