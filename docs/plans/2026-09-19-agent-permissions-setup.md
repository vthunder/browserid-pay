# Agent permissions: the checklist, and a one-pass setup

Written after provisioning carl and watching an agent ask for a standing
payer grant and a payee grant. Two things went wrong on the way; a third
was already wrong and nobody had noticed.

## 1. The permission surface today

An agent may hold warrants for five audiences. Everything is `{audience,
scopes}`; the broker mints them, and each audience enforces its own.

| # | audience | scopes | parameters | who enforces |
|---|---|---|---|---|
| 1 | the chain, `sbo+raw://avail:turing:506/` | `action:post`, `path:/agreements/**`, `path:/u/<email>/**`, `path:/communities/**` | — | the daemon, per write |
| 2 | the collection, `…/agreements/` | `contract:agreement` | `cap{amount,currency,window}`, `counterparties`, `max_duration`, `min_reputation` | the custodian, at lock |
| 3 | one agreement, `…/agreements/<id>/` | `contract:agreement` | same, narrowed to this deal | the custodian, at lock |
| 4 | the adjudicator's origin | `dispute` | — | the judge |
| 5 | the relay | `message` | — | the relay |

1 is "write to the ledger as me" and has no cap: it is how every object —
proposals, evidence, forum posts, attestations — gets signed. 2 is the
standing budget. 3 is the one-off fallback when 2 does not cover a deal.

## 2. What is actually enforced — and what is not

**The cap binds the payer only.** At lock the custodian reads the payer's
grant, checks `already + total > cap` across the window, and debits it
(`escrow.rs`). The payee's grant is *registered* so the custodian knows
which account to pay, and is never checked and never debited — there is a
test asserting exactly that (`spent == 0` on the payee's row).

So a payee's "standing permission up to $100 each" is not enforced by
anything. The card is real consent, and the agent should still honour it,
but no component will stop a payee agent entering a $10,000 agreement. The
asymmetry is defensible — a payee risks work, not money — but the card
currently reads as though it were a limit, and it is not.

**There is no per-agreement cap.** `cap` is a ceiling ACROSS agreements in
the window; that is deliberate (a "$200 a month" that meant "$200 each"
would be a trap). But it leaves "up to $100 each" with nowhere to live, so
an agent asked for both can only enforce the second as its own policy — as
carl's did, correctly and honestly, while telling the human it was doing so.

**`"*"` is not a legal counterparty.** The parameter takes exact emails or
`*@domain`; absent means anyone. The MCP schema description offers `"*" for
anyone`, which is wrong, and is why the first attempt was rejected. The
custodian also has an unreachable `c == "*"` branch.

**Standing consent does not bundle the relay grant.** `requestStandingConsent`
bundles the chain grant and the adjudicator, but not `extraGrants` — so the
first `send_message` after a "no more cards" standing grant still produces a
card. The two per-agreement paths do bundle it; this one was missed.

## 3. Fixes, smallest first

- [ ] Drop `"*"` from the `counterparties` description; say "omit for anyone".
      Remove the custodian's dead `"*"` branch or document it.
- [ ] Bundle `extraGrants` in `requestStandingConsent`, as the other two do.
- [ ] Add `max_per_agreement` to the `contract:agreement` parameters, enforced
      at lock alongside the window cap. This is a spec change (a new §5
      parameter, stricter = lower) — without it "up to $100 each" is a promise
      the agent makes on its own behalf.
- [ ] Make the payee's card say what it is. Either enforce the cap at accept
      (the SDK knows the total) or word the card as consent to be bound rather
      than a limit. Today it implies a ceiling nothing checks.

## 4. A `setup` playbook

The goal: one conversation, then ONE card. The agent asks the human for what
it cannot infer, then files a single request covering every audience.

Questions, in order, with defaults the human can accept silently:

1. **Will you spend?** If no, skip 2–4 (payee-only needs 1, 4, 5).
2. **Budget** — amount and window. Default: ask, no default.
3. **Per agreement** — a ceiling for one deal. Default: the budget.
4. **Who may you deal with?** Anyone / anyone at a domain / a named list.
   Default: anyone. Optionally a reputation floor instead.
5. **How long may an agreement run?** Default: none.

Then one `obtainWarrants` call carrying: the chain grant, the collection
grant built from 2–5, the adjudicator's `dispute`, and the relay's `message`.
That is the whole surface — five audiences, one card.

What must still interrupt the human afterwards, by design: an agreement that
exceeds the standing grant, one naming a different adjudicator, and any
dispute judgement. Those are authority the human has not given.
