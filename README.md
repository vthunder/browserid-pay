# browserid-pay

Agent payments on browserid-ng: a stateful **custodian** holds the human's
funds, enforces per-warrant spend caps, and settles onward (x402 first).
The agent never holds funds; the payee never learns about browserid.

Design: [docs/plans/2026-09-11-agent-payments-design-draft.md](docs/plans/2026-09-11-agent-payments-design-draft.md)

Depends on [browserid-ng](https://github.com/vthunder/browserid-ng).
