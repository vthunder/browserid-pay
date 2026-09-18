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

## 4. OPEN: the relay re-delivers old messages

Alice's `check_messages` returned bob's new message **plus messages from previous, finished agreements**. Suspected: no last-seen-id / no acknowledgement of receipt, so every call replays history.

- [ ] confirm whether `/api/receive` tracks a cursor per recipient
- [ ] if not, add one (or a client-side high-water mark)
- [ ] an agent re-reading settled agreements' chatter will eventually act on a stale instruction
