---
# browserid-pay-e1ou
title: Obligation timing derives from unverified payload timestamps
status: todo
type: bug
priority: critical
created_at: 2026-09-18T12:50:33Z
updated_at: 2026-09-18T12:50:33Z
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
