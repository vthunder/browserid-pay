# browserid-pay

Agent payments on browserid-ng: a stateful **custodian** holds the human's
funds, enforces per-warrant spend caps, and settles onward (x402 first).
The agent never holds funds; the payee never learns about browserid.

Design:
- [Payments](docs/plans/2026-09-11-agent-payments-design-draft.md) — custodian-enforced budgets over x402
- [Agreements](docs/plans/2026-09-15-agreements-design-draft.md) — contracts, escrow, adjudication

The [browserid principles](https://github.com/vthunder/browserid-ng/blob/main/docs/principles.md) apply here unchanged.

Depends on [browserid-ng](https://github.com/vthunder/browserid-ng).

## Layout

| crate / dir | what |
|---|---|
| `crates/custodian` | the custodian service (pay.dsp.fyi): accounts, bundle verification, per-warrant caps, ledger UI |
| `crates/genesis` | genesis / regenesis tooling for the browserid-pay SBO database |
| `deploy/` | runbooks (GENESIS.md) |
| `docs/plans/` | design drafts |

Build and test always through `ssh localtest` (see `Makefile`); the browserid-ng
crates come from the sibling checkout during development (`.cargo/config.toml`
patch) and from the pinned git source in Docker/CI.

## Custodian API (v1)

The custodian is a relying party for its own origin. Every payment is a
warrant presentation (`access_cert~assertion~warrant~config_cert`) whose
audience is the custodian and whose warrant carries a `pay:<rail>` scope
entry with a `cap` parameter — the budget the human signed.

| route | who | what |
|---|---|---|
| `POST /api/pay` | the agent | `{ presentation, amount, currency, payee, memo? }` → `{ receipt }`; errors `no_grant`, `cap_exceeded` (+`remaining`), `revoked`, `insufficient_funds`, `verification_failed`, `unsupported_currency` |
| `POST /api/login` / `GET /api/me` / `POST /api/logout` | the human (browser) | ledger UI session: balance, budgets (spent / remaining / window reset), receipts |
| `POST /admin/credit` | admin (`Authorization: Bearer $ADMIN_TOKEN`) | `{ identity, amount, memo? }` — test credit until a real rail lands |
| `POST /api/escrow/lock` | payer's agent | lock price + fees + contribution against a contract under a `contract:agreement` warrant (audience = the contract); see `docs/specs/agreement-v1.md` |
| `POST /api/escrow/<id>/acknowledge` · `/dispute` | a party's agent | release to the payee / freeze and hand back the sealed terms |
| `POST /api/escrow/<id>/ruling` | the named adjudicator | settle per the ruling (`payee` / `payer` / `split`), once |
| `GET /api/escrow/<id>` | anyone | escrow state |
| `GET /healthz` | anyone | liveness |

Rails: `transfer` (internal move to another account here). x402 and cards
are later rails behind the same cap check (`docs/plans/2026-09-11-…`).

Config (env): `PAY_ORIGIN`, `PAY_PORT`, `DATABASE_PATH`, `ADMIN_TOKEN`,
`BROKER`, `PAY_ACCEPTED_FALLBACKS` (default `browserid.me`), `PAY_CURRENCY`
(default `USD`), `PAY_SESSION_SECRET`; dev only: `PAY_DEV=1`,
`PAY_TRUST_ISSUER=domain=base64key[,…]` (pinned issuers, no status checks).
