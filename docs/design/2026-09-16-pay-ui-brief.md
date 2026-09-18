# pay.dsp.fyi — design brief for the account page and the demo view

**Date:** 2026-09-16 · for Claude Design · bundle: this file,
`current-pay-index.html` (the page as it is today), `sample-me.json` (the
data contract with realistic values), and the live services below.

## What this is

pay.dsp.fyi is the **custodian** in browserid-pay: it holds a person's
money, enforces the budgets that person signed for their AI agents, and
settles agreements between agents (escrow, fees, rulings). The page is
the person's window onto that. Everything on it is derived from three
things the person already did elsewhere: signed a budget or agreement card
at browserid.me, funded the account, and let their agents act.

Live: <https://pay.dsp.fyi> (sign-in with browserid), the public index
<https://index.dsp.fyi>, the adjudicator <https://judge.dsp.fyi>. Design
principles the product follows: <https://www.browserid.me/principles>.

## Who looks at it

1. **The account holder**, occasionally, to answer: *how much do I have,
   what have my agents been allowed to do, what are they doing right now,
   where did the money go, and is anything waiting on me?*
2. **A demo audience** watching two agents (alice's and bob's, both run by
   the presenter) agree, deliver, settle, dispute and get a ruling — live,
   over a few minutes. Today the presenter has to explain what happened
   from a table; the page should *show* it.

## What is wrong today (verbatim from the first live runs)

- **Budgets is always empty.** Only direct payments were recorded as
  budgets; the agreement grants (the "enter agreements up to $56 with bob"
  cards) never appeared. Fixed in the data (see contract below) — the design
  should show both kinds: standing budgets with spend bars, and
  per-agreement caps.
- **Escrows never leave.** Settled agreements sat in "Escrows" forever.
  The section should be **Agreements**, with escrow as *state* on an
  agreement (locked → disputed → settled), active ones first.
- **Receipts is not a ledger.** It showed one side of each movement (three
  −56 locks, two −50 settles) and none of the credits (+50 release, +5
  refund), so a +100 deposit "added up" to −168. The page needs a real
  **ledger**: every balance change with a running balance, and the
  agreement each row belongs to.
- In the demo both humans are one account, so the same person is payer and
  payee. The design must make a same-account agreement legible (role
  "both", net money movement) rather than assume two people.

## Information architecture (proposed)

1. **Account** — balance, prominent. Below it the ledger: date, what
   happened in words ("locked for agreement agr_… with bob", "released from
   agr_…", "adjudicator fee returned", "test credit"), amount signed,
   running balance. Rows link to the agreement.
2. **Agents & budgets** — one card per agent identity the person delegated
   (e.g. `danmills+alice@sandmill.org`). Inside: standing budgets ("may
   spend up to $20.00 every 30 days", spent / remaining bar, resets when,
   expires when) and per-agreement caps ("agreements up to $56.00 each
   with bob…", how many used). Revocation lives at browserid.me/account;
   link, don't duplicate.
3. **Agreements** — active first (locked, disputed: *what happens next*,
   e.g. "waiting for bob to deliver", "bob must respond by 10:42",
   "judge ruling pending"), then settled/ruled, collapsed. Each card: id,
   role and counterparty, the agents on both sides, price + fees +
   contribution, money moved (locked, released, refunded, fees,
   contribution), outcome and the judge's one-line reasoning when ruled,
   links to the public index entry and the judge case.
4. **Needs you** (top of page when non-empty): a dispute your agent must
   answer, a budget nearly spent, an approval pending at browserid.me.

## The demo view (brainstorm — pick one, or combine)

The page is also the presenter's stage. Ideas, roughly in order of payoff:

- **Agreement timeline.** Per agreement, a horizontal timeline: proposed →
  accepted → funded (escrow lock) → inputs handed off → receipt → delivered
  → settled, or → disputed → response → ruled. Each step: who, when, and
  the on-chain object it produced (linking to the index). Money as a
  second lane under it: −56 locked … +50 to bob, +5 back, 0.50 fee, 0.50
  contribution. Evidence objects (handoff, receipt, delivery, response) as
  chips with their commitment hashes; a revealed commitment turns "opened".
- **Two-column story.** alice's agent on the left, bob's on the right, the
  shared timeline down the middle, newest event highlighted; polls the
  custodian and the index every few seconds so it animates while the
  presenter drives the two terminals. This is the strongest demo artefact.
- **Money flow diagram** at settlement: one sankey-ish figure from the
  payer's account through escrow to payee / adjudicator / custodian /
  contribution fund. Small, static, per agreement.
- **Budget gauges** for the agents section: a ring or bar per budget,
  window countdown.
- **Ruling card**: the judge's outcome and reasoning, the split, what each
  side proved (opened commitments) — the "fairness" moment of the demo.

Non-goals: no admin tooling here, no key material, no revocation controls
(those stay at browserid.me), no charts that need a library.

## Constraints

- Static HTML + inline CSS/JS served by the custodian; no build step, no
  framework, no external assets. Must work at phone width. Light and dark.
- Data comes from `GET /api/me` (signed-in) and, for on-chain detail, the
  public index `GET https://index.dsp.fyi/v1/agreements/<id>` and the judge
  `GET https://judge.dsp.fyi/api/cases/<id>`. Nothing else.
- Identities are email-shaped strings and can be long
  (`danmills+alice@sandmill.org`); show the local part boldly, the domain
  quietly, never truncate the whole thing silently.
- Money is a decimal string with a currency; the demo is USD test credit
  and should say so once, quietly.
- Tone: plain words, no protocol jargon on the surface ("locked for an
  agreement", not "escrow warrant presentation"). Copy rules in
  `docs/principles.md` (browserid-ng) apply.

## Funding

The page models where money comes from as a list of **funding sources**, not
as "a deposit". A source is one of two modes, and the difference is when the
money moves:

- **`deposit`** — funds are pushed in ahead of time and sit as a balance:
  test credit today; a chain deposit address or a bank transfer later.
- **`charge`** — nothing sits here; the source is charged at the moment money
  is needed (a card charged when an escrow locks, an x402 settlement drawn
  against a card or an account elsewhere).

"Add funds" therefore opens a *list of sources*, with test credit the only
row today. A charge-mode source is a new row plus its own rail, never a
reshaped page: the dialog already renders "charged when needed" instead of an
amount field for such a source, and `GET /api/me` carries

```jsonc
"funding": {
  "test_credit_enabled": true,
  "sources": [ { "id": "test_credit", "kind": "test_credit", "mode": "deposit",
                 "label": "Test credit", "note": "…",
                 "available": "500.00", "max_per_request": "500.00", "ceiling": "1000.00" } ]
}
```

Test credit is self-serve: `POST /api/deposit { amount }`, authenticated by
the **signed-in session** (never the admin token), gated by `PAY_TEST_CREDIT`,
bounded per request (`PAY_TEST_CREDIT_MAX`), by a per-account ceiling
(`PAY_TEST_CREDIT_CEILING`) and by a one-a-minute cooldown. It writes the same
`credit` ledger row and receipt as the admin route. Turning the flag off
removes the button and the route together.

## Data contract — `GET /api/me`

```jsonc
{ "email": "danmills@sandmill.org", "currency": "USD", "balance": "113.00",
  "ledger": [ { "at": 1789590000, "delta": "+5.00", "balance_after": "113.00",
                "kind": "escrow_refund", "agreement_id": "agr_…", "memo": null } ],
  "agents": [ { "identity": "danmills+alice@sandmill.org",
                "grants": [ { "scope": "contract:agreement", "cap": "56.00", "spent": "56.00",
                              "remaining": "0.00", "agreement_id": "agr_…",
                              "counterparties": ["danmills+bob@sandmill.org"], "exp": 1792000000 },
                            { "scope": "pay:transfer", "cap": "20.00", "window_secs": 2592000,
                              "spent": "0.00", "remaining": "20.00", "window_resets_at": 1791000000, "exp": 1792000000 } ] } ],
  "agreements": [ { "id": "agr_…", "state": "settled", "role": "both",
                    "counterparty": "danmills+bob@sandmill.org", "my_agent": "danmills+alice@sandmill.org",
                    "their_agent": "danmills+bob@sandmill.org", "adjudicator": "danmills+judge@sandmill.org",
                    "price": "50.00", "fees": { "custodian": "0.50", "adjudicator": "5.00" },
                    "contribution": { "amount": "0.50", "recipient": "fund@dsp.fyi" },
                    "locked_at": 1789580000, "settled_at": 1789590000, "outcome": "ruled:split",
                    "money": { "locked": "56.00", "released_to_payee": "30.00", "refunded_to_payer": "20.00",
                               "fees_paid": "5.50", "contribution_paid": "0.50" },
                    "links": { "index": "https://index.dsp.fyi/v1/agreements/agr_…",
                               "judge_case": "https://judge.dsp.fyi/api/cases/agr_…" } } ],
  "receipts": [ /* per-payment records, each with agreement_id when escrow-related */ ] }
```

Ledger `kind` values: `credit`, `pay`, `receive`, `escrow_lock`,
`escrow_release`, `escrow_refund`, `adjudicator_fee`, `custodian_fee`,
`contribution`. Agreement `state`: `locked`, `disputed`, `settled`;
`outcome`: `acknowledged` or `ruled:<payee|payer|split>`.

The index entry (public) adds the on-chain timeline: proposal, acceptance,
lock, evidence objects (`handoff`, `receipt`, `delivery`, `dispute`,
`response`) with block numbers and commitments, and the ruling with its
reasoning. The judge case adds `respond_by_block` (a BLOCK HEIGHT, not a
time — render it against the chain head) and the response with
`matched` flags per reveal.

## What to deliver

Mockups for: the account page (three sections + "needs you"), the
agreement card in each state, and the demo timeline view (two-column story
preferred). Phone and desktop. Copy included, using the words above.
