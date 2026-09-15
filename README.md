# browserid-pay

Agent payments on browserid-ng: a stateful **custodian** holds the human's
funds, enforces per-warrant spend caps, and settles onward (x402 first).
The agent never holds funds; the payee never learns about browserid.

Design:
- [Payments](docs/plans/2026-09-11-agent-payments-design-draft.md) — custodian-enforced budgets over x402
- [Agreements](docs/plans/2026-09-15-agreements-design-draft.md) — contracts, escrow, adjudication

The [browserid principles](https://github.com/vthunder/browserid-ng/blob/main/docs/principles.md) apply here unchanged.

Depends on [browserid-ng](https://github.com/vthunder/browserid-ng).
