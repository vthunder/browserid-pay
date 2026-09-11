# Agent payments: custodian-enforced budgets (design draft)

**Date:** 2026-09-11 · **Status:** review draft 
Discussion with Dan, 2026-09-11. Nothing below is implemented.

## The one idea

An agent's wallet cannot enforce a budget on itself (agents run embedded
wallets — self-enforcement is dead on arrival). So the budget is enforced by
**whoever holds the money**. That party verifies browserid bundles, so it is
the relying party; the payee is not. We call it the **custodian**.

- The human funds an account at a custodian. The agent never holds funds.
- A payment is an ordinary warrant presentation to the custodian: grantor =
  the human, grantee = the agent, **audience = the custodian** (the audience
  is whoever verifies; the payee never does), scopes = `pay:*` with a cap
  parameter. *Where* the agent may spend is a constraint inside the grant,
  never the audience — so one warrant is one budget across many sites. The custodian is stateful by nature, so the cap is
  **audience-enforced** — the stateful-parameter door in
  `docs/warrant-use-cases.md` (Known limits) opens here without touching
  wallets or stateless verifiers.
- The custodian settles onward on whatever rail the payee accepts. First
  rail: x402 (USDC). Cards are the later fiat rail behind the same audience.
- Same revocation as everything else: the account ledger row's bit kills the
  agent's *next* payment; the human's own funds and keys are untouched.

The custodian is scaffolding in the sense of principles §7: browserid.me runs
one so it works on day one; anyone can run one, because the audience is just
a DNS-rooted name. It runs on a **separate domain** (not a subdomain):
compromise of the money service must not reach issuance or the registry.

## Flow 1 — fund and grant

1. Human signs in to the custodian (ordinary browserid login) and deposits
   testnet USDC from their own chain wallet to the custodian's deposit
   address for their account. (Deposit, not faucet: honest about the real
   flow.)
2. The agent asks for authority through the existing out-of-band lane: a
   `warrant` request filed at the registry, with
   `grants: [{ audience: "https://pay.example", scopes: [ { scope: "pay:x402",
   cap: { amount: "20.00", currency: "USD", window: "P30D" } } ] }]`.
3. The consent card renders the cap in the wallet-owned label table:
   "**let agent@… spend up to $20.00 every 30 days** at pay.example, paid from
   your account — signed automatically". No requester prose.
4. The human's config cert signs the warrant; it is registered (status index,
   ledger row) and delivered to the agent's poll, exactly as agent grants are
   today (use case 2). Nothing new on this path except the scope label.

## Flow 2 — pay

1. The agent requests a resource; the server answers `402` with x402's
   `accepts[]` (scheme, network, asset, `maxAmountRequired`, `payTo`,
   `resource`).
2. The agent's wallet calls `pay`: it mints a fresh access cert + assertion
   (`aud` = custodian) and POSTs `{ bundle, challenge }` to the custodian's
   `/pay` endpoint. The challenge is the 402 body, verbatim.
3. The custodian verifies the bundle (§6.1, fail-closed status checks), then:
   - the warrant's audience is itself and a `pay:x402` entry is present;
   - `challenge.maxAmountRequired`, converted at the custodian's published
     rate, fits the **remaining** cap for this warrant in the current window
     (per-warrant counter keyed by `status.idx` — the warrant's identity);
   - the account balance covers it.
4. The custodian signs the EIP-3009 transfer from the human's account
   address to `payTo`, debits the account, and records a **receipt**
   `{ grantor, grantee, warrant status ref, amount, asset, payTo, resource,
   time, tx }` in the account ledger. It returns the `X-PAYMENT` header
   value to the agent.
5. The agent retries the request with the header; the payee's facilitator
   settles. The payee learns nothing about browserid — it sees a normal
   x402 payment.

Refusals: `no_grant`, `cap_exceeded` (with remaining amount), `revoked`,
`insufficient_funds`, `unsupported_scheme`. All 4xx, all logged to the
custodian's ledger so the human sees an agent hitting its limit.

## The `pay:` scope namespace

`pay:<rail>` names what the custodian may be asked to settle. `pay:x402` is
the first value; `pay:card` reserved. Parameters are attenuations with
stricter-wins order (spec §5, scope entries), all audience-enforced:

| parameter | meaning | stricter |
|---|---|---|
| `cap` | `{amount, currency, window}`; `window` an ISO-8601 duration, **rolling** from the warrant's `iat`, or absent for a lifetime cap | smaller amount, shorter window |

That is the whole vocabulary for now. A richer one is foreseeable —
per-payment ceilings, stacked per-day/week/month caps, payee allowlists,
merchant categories (possibly dynamic) — and it slots in as more parameters
on the same entry, stricter-wins. Deliberately not designed yet (Dan:
premature). A custodian MUST refuse an entry carrying a parameter it does
not implement (§6.6 invariant 14).

The cap counter is per warrant, not per agent: two warrants at the same custodian are two budgets, two ledger rows, two kill
bits — same reasoning as one audience per record.

## What the human sees

Two surfaces, deliberately split. **browserid** (/account) shows what it
shows for every warrant: the row, the scope label ("up to $20.00 every 30
days"), and the revoke button — nothing about money moving. **The custodian**
has its own UI: balance, deposits, spent / remaining / window reset per
warrant, and every receipt. browserid never learns balances or receipts.

## What the agent side grows

A **separate package** (working name `@browserid-ng/pay`) depending on the
agent SDK: one MCP tool / one function, `pay`, taking the 402 challenge and
returning the payment header; it uses the stored warrant and the existing
`get_assertion` path. The identity wallet stays money-free.

The demo: an agent buys a paid resource from an x402 server, the human revokes at /account, the next purchase fails. The
"revoke kills the agent" story, with money.

## Deferred (one line each, in order of likely need)

- **Delegation.** An orchestrator slices its budget to subagents; the
  custodian charges the chain. Needs bean `0ijs` (what an agent signs
  sub-warrants with) plus a child-cap ≤ parent-cap rule. Enforcement is
  trivial once the custodian is stateful; the record format is the work.
- **Payee attribution.** A receipt to the payee naming the human, gated by
  an additional scope. No payee speaks it today; theoretical until one does.
- **SBO tie-in.** Receipts stay private in the custodian ledger. On chain:
  receipt *commitments* (hash + time) for later proof in a dispute, and
  custodian *attestations* about an agent (settled count, disputes, account
  age — the Attestation spec's shape) for reputation without per-payment
  leakage. Multiple custodians ⇒ no single reputation oracle.
- **Contracts.** Two agents negotiate, then register a contract that
  payments reference by SBO URI. Open question (Dan): parties may want the
  contract private too — so the on-chain object may be a commitment to the
  contract, with the terms revealed only to an adjudicator. Needs thought
  before the public-contract shape is assumed.
- **Fiat rail.** `pay:card` via a card-issuing API, same audience, same cap.
- **Spend vocabulary.** Per-payment ceilings, stacked windows, payee
  allowlists, categories — more parameters on the `pay:` entry when a real
  case needs one.

## Settled in review (2026-09-12)

- Cap in USD, settlement in USDC at the custodian's published rate: fine
  for USDC; revisit for volatile assets.
- Windows are rolling from the warrant's `iat`.
- `pay` lives in a separate package, not the identity wallet.
- Spend UI lives at the custodian, not browserid.
