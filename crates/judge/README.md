# judge — the browserid-pay adjudicator

An AI judge with its own browserid identity (bean `browserid-pay-6rcb`). Wire
shapes: `docs/specs/agreement-v1.md`.

## Endpoints

| route | who | what |
|---|---|---|
| `POST /api/dispute` | either party's agent | `{ presentation (audience = this origin), contract_id, sealed_terms, note? }` → `202 { state: "open" }`. Verifies the caller, unseals the terms, checks the commitment against the on-chain proposal, that the caller is a named party and this judge is the named adjudicator, then rules in the background. `409 already_filed` on a second filing. |
| `GET /api/cases/<id>` | anyone | `state` (`open` → `ruled` → `delivered`, or `failed`), the ruling, the chain submission id, any delivery error |
| `POST /api/cases/<id>/deliver` | anyone | retry delivering a ruling the custodian did not accept |
| `GET /api/setup` | operator | provisioning status: identity, warranted audiences, or the pending approval URL / code / fingerprint |
| `GET /` , `GET /healthz` | anyone | service page (identity, price, sealing key), liveness |

## How a ruling happens

1. `rule` — Claude (`JUDGE_MODEL`, default `claude-fable-5-1`, effort high,
   server-side refusal fallbacks) gets the terms, the proposal, the evidence
   objects (only those authored by the parties), and the dispute note, and
   must answer with a strict JSON ruling. `llm::validate` rejects anything
   off-vocabulary or a split that does not sum to the price.
2. The ruling object is signed and posted to `/agreements/<id>/` (ID
   `ruling`, `agreement.ruling.v1`) as this identity.
3. `agreement:ruled` attestations are posted on both parties under
   `/<judge>/attestations/<party>/agreement:ruled`.
4. The ruling is delivered to the custodian named in the proposal:
   `POST <custodian>/api/escrow/<id>/ruling` `{ presentation (audience = the
   custodian origin), ruling }`.

## Identity and provisioning (one-time, needs a human)

On first boot with no `judge-credential.json` in `JUDGE_DATA_DIR`, the judge
starts a **merged provisioning request** at `BROKER` (browserid.me): one
approval covers the identity and the bundled warrants —

- audience `sbo+raw://avail:turing:530/` with scopes `action:post`,
  `path:/agreements/**`, `path:/providers/adjudicators/**`,
  `path:/*/attestations/**` (chain writes), and
- one warrant per custodian in `JUDGE_CUSTODIANS` with scope `ruling`
  (delivery).

The approval URL, user code, and key fingerprint are logged and served at
`GET /api/setup`. Open the URL signed in as the identity rulings should be
attributed to (`judge@dsp.fyi` if that mailbox exists; otherwise whatever
identity approves becomes the judge's, and the provider listing id follows
it). The pickup is persisted (mode 600) and the service provisions the
provider listing `/providers/adjudicators/<identity>` on the chain.

Chain writes are signed with the ephemeral **access key** the IdP certifies
and carry the four-object presentation as `Auth-Cert` (the daemon's
device-attribution path; same as mingo's `sign_as_logged_in_user`). There is
no separate long-lived SBO signing key. The **sealing key** (X25519, sealed
box) is generated on first boot into `judge-seal-key.json` and published in
the listing.

Adding a custodian later means a new provisioning (delete the credential
file and re-approve) until a warrant-request flow is added here.

## Config

`JUDGE_ORIGIN`, `JUDGE_PORT`, `JUDGE_DATA_DIR`, `BROKER`, `JUDGE_HANDLE`,
`JUDGE_CHAIN_AUDIENCE`, `SBO_DAEMON`, `JUDGE_CUSTODIANS` (comma list),
`JUDGE_ACCEPTED_FALLBACKS`, `JUDGE_PRICE`, `JUDGE_CURRENCY`, `JUDGE_MODEL`,
`ANTHROPIC_API_KEY`, `AVAIL_NETWORK` / `AVAIL_TURBODA_KEY` (chain
submissions; absent ⇒ writes are logged and dropped). Dev only: `JUDGE_DEV=1`,
`JUDGE_TRUST_ISSUER=domain=base64key`.

## Deploy

`ghcr.io/vthunder/browserid-pay/judge` via `.github/workflows/deploy-judge.yml`;
declared in sandmill-infra `apps/hobby/judge.conf` + `secrets/judge.env.age`;
`make release-judge` releases HEAD's image by hand (mini-ops key).
