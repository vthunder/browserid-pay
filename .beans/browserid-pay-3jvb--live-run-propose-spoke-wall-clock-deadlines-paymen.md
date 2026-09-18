---
# browserid-pay-3jvb
title: 'Live run: propose spoke wall-clock deadlines; payment obligation, next_step and relay gaps'
status: todo
type: bug
priority: high
created_at: 2026-09-18T21:21:28Z
updated_at: 2026-09-18T21:21:28Z
---

Found by the first live run on 506 (2026-09-18), by the agents themselves — bob refused to accept terms he judged defective, and alice then could not express the fix. Both diagnoses were correct.

## 1. FIXED: the MCP `propose` tool still spoke wall-clock deadlines

`obligations[].deadline: z.number()` survived the move to block heights (`browserid-pay-e1ou`). The SDK bounds an obligation only when `deadline_block` is absent, so a `deadline` an agent passed was **silently ignored** — and carried into the terms as a junk field.

Two visible consequences, both reported verbatim by the agents:
- *"The tool ignored my block number… it reads that field as a clock time"* — alice passed a height into `deadline`, got the default window instead, and the height sat in the terms as a stray clock field.
- *"Bob's deadline is still the same as yours"* — with every `deadline` ignored, EVERY obligation fell back to the same `head + P7D` default. An input and the deliverable that depends on it ended up due at the same block, so the performer had zero time.

**Fixed:** `deadline` is now **rejected with a message that says what to use instead** (rename-never-reinterpret, as everywhere else in that change). Added:
- `deadline_block` — an explicit height
- `deadline_in` — an ISO-8601 duration (`"PT24H"`), resolved against the **current head at propose time**, so each party gets its own deadline

`windowBlocks` / `windowSecs` / `NOMINAL_BLOCK_SECS` are now exported from the SDK barrel (`windowBlocks("PT24H") === 4320`).

## 2. OPEN: nothing adds a payment obligation

Bob: *"There's no payment obligation… nothing forces her to fund it."*

He is right that it is absent, and right that it matters: `deriveObligations` treats `kind: "payment"` as met by the escrow lock, so without one, funding is not represented in the obligations at all — no state, no deadline, no lateness. The proposal names a payer and the escrow still works, but the terms never say *when* funding is due.

Nothing in `propose` adds one and nothing in the tool description suggests it. Earlier hand-written demo terms included one, which is why this is the first time it has been missed.

- [ ] should `propose` synthesise a payment obligation from `price` when the caller omits one? It has everything it needs (payer, amount)
- [ ] if synthesised, what deadline — no later than the first input handoff seems right, which is what bob asked for
- [ ] or keep it explicit and make the tool description demand it. Silently optional is the one option to rule out

## 3. OPEN: `next_step` does not surface a waiting relay message

Alice ran `next_step` on a 60s loop and **never noticed bob's counter-offer**. She only found it when told to check messages explicitly.

`next_step` drives the agreement lifecycle off chain state; an inbound relay message is not chain state, so it is invisible. For a proposal awaiting a counterparty, "they replied" is the single most important next action.

- [ ] `next_step` should check for unread relay messages and surface them
- [ ] a proposal in `proposed` with an unread message from the acceptor should recommend reading it, not waiting

## 4. FIXED: the relay cursor was collateral damage from clearing old terms

The first diagnosis here ("no last-seen-id") was **wrong**. A cursor exists, is persisted, and advances — `{"last_seen": 33}`.

The real defect is **where it lived**: `join(TERMS_DIR, "relay-cursor.json")`, i.e. inside the per-agreement terms directory, indistinguishable from one more disposable terms file. `readCursor` falls back to **0** on any read failure, so losing it silently replays every message the agent has ever received.

I caused the loss: while prepping the live run I described `~/pay-clients/*/state/terms/` as "stale terms for 530 agreements, harmless clutter" and suggested clearing it. That directory held the cursor.

**Fixed:** the cursor moved to the state root, beside the terms directory rather than inside it. The legacy path is still read (`max` of both), so an existing cursor survives the upgrade, and a failed write now logs instead of failing silently.

**Replay-on-missing stays** (Dan, 2026-09-18). The cursor is client-side only, so the relay keeps no record of what it delivered — which makes an absent cursor indistinguishable from a brand-new client. Replaying is then the only default that cannot silently drop a message; the cost is noise the agent can reason through. Revisit only if a server-side per-recipient cursor ever exists.

- [x] on a genuinely absent cursor, replaying everything is correct — see above
- [ ] note the consequence: a SECOND client for the same identity (another machine) also replays from zero, and two clients advance separate cursors. Fine today, wrong if an identity is ever run from two places at once
