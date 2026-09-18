---
# browserid-pay-e1ou
title: Obligation timing derives from unverified payload timestamps
status: completed
type: bug
priority: critical
created_at: 2026-09-18T12:50:33Z
updated_at: 2026-09-18T16:33:43Z
blocking:
    - browserid-pay-b3gq
---

**Every timing decision in the system is currently made from a timestamp the interested party wrote itself, and nothing checks it.**

## The chain of findings

1. **Evidence objects carry a self-asserted `at`.** `sdk/agreements/src/agreements.mjs:464`:
   ```js
   const ev = { v: 1, id, kind, by: this.email, at: nowS(), ...extra };
   ```
   `nowS()` is the author's own `Date.now()`. The daemon does not validate payload fields, so this is an unconstrained claim.

2. **`deriveObligations` computes lateness from it.** Receipts use `r.at` (:1155), the answer anchor uses `lastAnswer.at` / `anchor.at` (:1191, :1193), deadline sliding uses `ds.at` (:1202), and the response deadline is `anchor_at + respondSecs` (:1215). Met/late/unanswered all follow from those.

3. **The judge hands the raw payload to the LLM.** `crates/judge/src/main.rs:264` builds each evidence entry as `{ id, by, confirmed, object }` where `object` is the payload verbatim — so `at` reaches the model, presented under a system prompt that calls this *"the on-chain record"*.

4. **The judge passes no chain time at all.** `ObjectView` (`crates/chain/src/daemon.rs:13`) is `{path, id, creator, value, confirmed}` — no block number, no inclusion time. Ordering is whatever `/v1/list` returned, unsorted.

5. **But the prompt tells the model to reason about timing anyway.** `crates/judge/src/llm.rs`: *"Read the evidence in chain order"* and *"Weigh WHEN each claim was made, which the chain records"*. The chain records it; the judge never sends it. The only timing information actually in context is the forgeable `at`.

6. **And the output feeds reputation.** `late_party` comes back from the ruling and becomes an attestation.

## The attack

Deliver late; set `at` to any time before the deadline. `deriveObligations` marks the obligation met on time, and the adjudicator — reading `at` as chain-recorded fact — agrees. No signature check fails, because the field was never signed *as a claim about time*; it is just payload.

A second variant is worse because it costs the victim money: the response window is derived as `anchor_at + respondSecs`. **Backdate the anchor and the counterparty's window shrinks**, potentially closing before they can answer, which drives an `unanswered` obligation to an adjudicator. That is precisely the griefing vector `browserid-pay-b3gq` removed negotiable windows to prevent, reachable by another route.

## Fix, in order

1. **Stop deriving obligations from payload `at`.** This is the core defect; everything else is downstream.
2. **Surface chain time to the judge and the SDK.** The daemon already stores it — `StoredObject.block_number` (`sbo-core/src/state/objects.rs:29`) — it is simply dropped by `DaemonReader::view`, which selects five fields and discards the rest. Check whether `/v1/list` already serializes it; if so this is a small change on our side. Inclusion time is the authoritative anchor because no party controls it.
3. **Remove `at` from evidence payloads** rather than leaving a plausible-looking field nobody should trust (Dan: no accidental foot-guns). If a party's own claimed time is genuinely wanted, name it so nobody can mistake it — `claimed_at`, explicitly labelled unverified wherever it is displayed or prompted.
4. **Add an HLC header to agreement writes.** Self-asserted but *bounded* by `W` (5 min default), which makes it strictly better than an unbounded payload field, and it gives sub-block resolution. Secondary to (1) and (2): HLC constrains a claim, inclusion time is a fact. Use inclusion time for deadline evaluation and HLC for resolution within a block.
5. **Fix the judge prompt** to match what it is actually given: state the precision bound, and say plainly that any payload time field is a party claim, not a record. Today the prompt asserts a capability the inputs do not supply, which invites confident timing conclusions from nothing.

- [ ] audit every consumer of `at` in the SDK and judge before changing its meaning
- [ ] confirm `/v1/list` exposes `block_number`; if not, that is an sbo-side change
- [ ] block number → time: is a block's timestamp available, or only its height? Height alone gives ordering, not deadlines
- [ ] `deriveObligations` is used by both parties' clients — both must derive the same view, so the switch has to be simultaneous or versioned
- [ ] re-check `browserid-pay-b3gq`'s floors once timing is trustworthy

## Cross-stack investigation (2026-09-18): what is actually available

**Block height is available today; block *time* is not.**

- `sbo-daemon/src/main.rs:276` already exposes `block: obj.block_number` in the object JSON. Our `DaemonReader::view` (`crates/chain/src/daemon.rs:47`) simply does not read it. **Height is a one-line fix on our side** and gives trustworthy *ordering* immediately — which is what the judge's prompt already claims to have ("read the evidence in chain order").
- **Time is not stored.** The daemon reads the DA block's `timestamp.set` inherent at `sync.rs:571` and threads it into `L2Context::for_block` for attribution and the HLC bound — then drops it. `StoredObject` keeps `block_number` and no timestamp. Filed as **`sbo-xgyr`** (high): persist and expose it.

So the fix splits cleanly, and the first half is not blocked:

1. **Now, in this repo:** read `block` into `ObjectView`, sort evidence by it, and pass it to the adjudicator. Stop the prompt claiming an ordering it was never given.
2. **Then, once `sbo-xgyr` lands:** switch deadline evaluation to inclusion time.
3. **Independently:** remove `at` from evidence payloads, and add an HLC to agreement writes for sub-block resolution.

Note `main.rs:680` builds `L2Context::for_block(Some(now), ..)` on the local-submit path — that is the daemon's own clock, not block time. Whatever `sbo-xgyr` persists must come from the replay path, not that one.

## How the rest of the stack handles time, for comparison

**mingo does it properly-ish** (`mingo-jlo1`): every write carries an HLC (`app.js:473`, `:858`), display and sorting read it, and it is therefore *bounded* by `W`. Its spaces set `W = 86400`, so a post can claim to be 24h old — deliberate, since the seed corpus needs it — and the residual gaming (forward-dating by `ε` to top a sort) is cosmetic at human volume.

**The contrast is the lesson.** mingo carries a bounded claim and treats it as a claim. We carry an *unbounded* claim in a payload field and treat it as a fact. Neither uses inclusion time, because until `sbo-xgyr` nobody can.

## Proposed fix (2026-09-18, Dan): block heights as the agreement clock

**Every contractually meaningful time becomes a block height** — obligation deadlines, proposal expiry, the delivery response window, the adjudicator's response window, the backstop. Wall-clock survives only where it is not binding: warrant credential lifetimes, polling schedules, and display estimates.

Full analysis and a component-by-component plan: **`docs/plans/2026-09-18-block-height-timing-plan.md`**.

**Why this rather than a partial fix:** it removes the class instead of shrinking it. Height is not a claim, so `W`, skew tolerance, HLC and clock trust all stop mattering for deadlines. Both sides derive the same view by integer comparison of a number neither controls — which `browserid-pay-b3gq` requires and today merely hopes for.

**And it does not wait on `sbo-xgyr`.** `GET /v1/state-root` returns the head `block`, and `/v1/list` already returns `block` per object (`sbo-daemon/src/main.rs:276`) — our `DaemonReader::view` simply does not read it.

**A correctness property worth noting:** delivering is an on-chain write, so under wall-clock deadlines a chain stall makes a party late when they could not possibly have performed. Under height deadlines the deadline cannot advance while blocks are not produced. And since Substrate skips slots rather than numbers, and blocks cannot be produced faster than the slot rate, a height-derived window is always **≥** its intended wall-clock duration — a party can be given more time than intended, never less.

**The bug, stated precisely.** `docs/specs/agreement-v1.md:114` already says *"Every evidence object also carries `at` (writer-claimed unix time; **chain order is the truth**)."* The spec was right all along; the implementation does the opposite. This restores it.

**Smallest useful step, worth doing even if the rest slips:** make the judge sort evidence by block and pass heights, and fix the prompt. That removes the adjudicator's exposure to the forged field without any format change.

**Biggest risk, from the plan's open questions:** a height and a unix timestamp are both integers, so an un-migrated deadline **fails silently** — read as a height it is a number in the distant past and every obligation is instantly late. The change needs an explicit version discriminator, not a reinterpretation.

## Summary of Changes

Closed. Every timing decision in the system now rests on a block height, which the chain assigns and no party controls.

| | |
|---|---|
| `browserid-pay-wl7v` | `ObjectView.block` + a head read; `head_block()` required, never defaulted |
| `browserid-pay-dtjt` | judge orders evidence by block, response window becomes a height, prompt rewritten |
| `browserid-pay-031y` | rename rather than version — `deadline_block`, `expires_block` |
| `browserid-pay-1srw` | `deriveObligations` and the whole SDK; `at` deleted |
| `browserid-pay-bh6u` | the custodian boundary: party-supplied times converted, custodian-observed ones not |
| `browserid-pay-usj3` | the backstop bug the conversion exposed |
| `browserid-pay-x9an` | do NOT add an HLC — heights removed the premise |

**What the fix actually is.** `docs/specs/agreement-v1.md` always said *"`at` is writer-claimed; chain order is the truth"*. The implementation did the opposite. This restored the spec rather than inventing a new model.

**What it found on the way.** Converting the units forced every time comparison to be looked at, and two of them were already broken independently of this bug: the abandonment backstop had been firing at its 1-day floor on every escrow because `latest_deadline` was never sent (`usj3`), and `max_duration` on standing consent was unenforced for the same reason. Neither had a failing test — `backstop_due` was correct and its unit test passed while nothing populated the field it read.

**The rule that came out of it**, applied at every boundary: **rename, never reinterpret.** A height and a unix timestamp are both integers, so nothing fails loudly on its own. The rename was applied to struct fields, SQL columns, an env var, API fields and terms fields — and each one caught something a version flag would have missed.

**Still open:** `demo/run-local.mjs` is updated and parses but needs a live daemon, so the end-to-end path is unverified. That is the main thing to run before trusting this.
