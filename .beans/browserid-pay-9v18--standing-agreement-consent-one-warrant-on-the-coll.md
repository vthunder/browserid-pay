---
# browserid-pay-9v18
title: 'Standing agreement consent: one warrant on the collection, not one per agreement'
status: completed
type: feature
priority: high
created_at: 2026-09-16T23:59:02Z
updated_at: 2026-09-17T00:02:17Z
---

Every agreement needs its own consent card, because `contract_audience` is per-agreement:

    sbo+raw://avail:turing:530/agreements/<id>

A warrant is minted for one audience, so it can structurally only ever cover one agreement. In a live run bob needed three approvals — provision, relay, contract — and the contract one returns for every agreement forever. That is what stops two looping agents from looking autonomous.

Most of the enforcement already exists. The custodian `covers()` (`escrow.rs`) already understands blanket counterparties — `"*"`, `"*@domain"`, or a list — and `cap` / `min_reputation` / `max_duration` are already scope parameters it checks at lock. What is missing is an audience that is not tied to one agreement, and cap accounting over a window.

## Design (Dan, 2026-09-17)

- **Standing audience is the COLLECTION**, `<repo>/agreements` — the ledger, not one custodian. Keeps contracts addressable by the ledger if a second custodian ever exists.
- **Per-agreement warrants stay.** Both forms are valid and compose: a blanket grant for agreements under $50, plus a one-off approval for a specific $100 agreement, without raising the blanket cap. An agent that finds the standing grant insufficient asks for the one-off rather than a bigger blanket.
- The SDK prefers the narrowest authority it already holds; it only asks when it holds neither.
- The consent card must say plainly how broad a standing grant is — "enter agreements up to $200.00 every 30 days with anyone at sandmill.org" is a much larger grant than one $56 agreement.

## Scope

- [x] SDK: collection audience helper; `#contractWarrant` prefers per-agreement, then standing, then requests
- [x] SDK: a way to request a standing grant (cap + window + counterparties + optional min_reputation)
- [x] MCP: a tool to ask for standing consent, and instructions on when to offer it
- [x] Custodian: accept a presentation for either audience at lock (per-agreement first, then collection)
- [x] Custodian: rolling-window cap accounting — sum prior locks under the same warrant_key within the window (escrows already carries `warrant_key` and `locked_at`)
- [x] Custodian: `grant_row.window_secs` carries the real window so the account page shows spent/remaining
- [x] Tests both sides, including a standing grant that covers one agreement and is exhausted by the next

## Summary of Changes

**Custodian.** `COLLECTION_AUDIENCE` (`sbo+raw://avail:turing:530/agreements`) alongside the per-agreement one. `caller_for_contract` verifies against the agreement audience first and falls back to the collection, so each party chooses independently — the payer may act under a blanket grant while the payee approved this one deal. A revocation is never masked by an audience miss.

`ContractGrant` gained `window_secs` from the cap, and `grant_row` now carries it so the account page shows spent/remaining rather than a bare cap.

The cap now binds **across** agreements under the same warrant: `db::locked_since` sums prior locks by `warrant_key` within the rolling window, and the check moved **inside the insert transaction** — outside it, two locks racing could both read a stale total and both pass. Without this a standing "$200 a month" would silently have meant "$200 per agreement".

**SDK.** `collectionAudience()`; `#contractWarrant` prefers authority already held, narrowest first — a one-off for this agreement, then the standing grant, then asks. Reaching for the standing warrant is a convenience, not a bypass: the custodian re-checks cap and counterparties at lock, so a grant that does not actually cover the deal is refused there.

`requestStandingConsent({cap, window, counterparties, min_reputation, max_duration})` asks once, explicitly — nothing requests it implicitly, because the card is far broader than one agreement.

**MCP.** `request_standing_consent` tool, and instructions to offer it when the human describes ongoing dealings, to say plainly how broad it is, and to fall back to a one-off card for a deal the blanket will not cover.

Blanket counterparty forms needed no work: `covers()` already understood `"*"`, `"*@domain"` and explicit lists.

**Tests** (20 custodian, 20 SDK, local e2e green): a standing grant covering two agreements and refusing the third with how much is left; a one-off warrant working alongside a standing one too small for the deal; a standing grant still honouring `*@domain` counterparties.

## Effect

Cold start is provision + relay + one standing card. Every agreement after that needs **zero** approvals — which is what makes two looping agents actually look autonomous.
