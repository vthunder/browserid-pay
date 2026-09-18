---
# browserid-pay-4v59
title: Move agreements to chain 506 and abandon 530
status: in-progress
type: epic
priority: high
created_at: 2026-09-18T15:07:43Z
updated_at: 2026-09-18T18:12:27Z
---

**Decision (Dan, 2026-09-18): abandon app 530 and run agreements on 506, alongside mingo.** One chain for the forum and for agreements. The agreements currently live on 530 do not need to keep working.

This was deferred in `docs/plans/2026-09-18-agent-forum-design.md` §6 and is now decided. It also collapses the migration question in the block-height timing plan into a **hard cutover with no interop code** (`browserid-pay-031y`).

## What has to move onto 506's root policy

506 already has, in the same shape: `/sys/names/*` create + owner-update, the DNSSEC self-auth fragment, and `admin` (sys key) with `post, transfer, delete, govern` on `/**`. So the sys key can install all of this by **governed update — no regenesis**.

Missing, from `crates/genesis/src/main.rs:314-368`:

**Grants**
- `{ to: "*", can: ["post"], on: "/providers/custodians/$user/**" }`
- `{ to: "*", can: ["post"], on: "/providers/adjudicators/$user/**" }`
- `{ to: "*", can: ["create"], on: "/agreements/**" }`

**Restrictions**
- `{ on: "/providers/**", require: { schema: "provider.v1", max_size: 16384 } }`
- `{ on: "/agreements/**", require: { max_size: 65536 } }`
- the contribution floor: `on: "/agreements/*/"`, `id: "proposal"`, with the `fn`/`amount`/`percentage` field conditions

**Plus** the `/sys/contribution` object and the provider listings for the custodian and adjudicator, re-posted on 506.

## The user namespace: adopt `/u/**` (decided, Dan 2026-09-18)

One convention on the chain, not two. Since all app data is being wiped there are no historical objects to move — this is a path change, not a migration, so do it as part of this epic rather than after.

### What `$user` and `$owner` actually mean

From `sbo-core/src/policy/path.rs:13-15`:

- **`$user`** — the acting signer's canonical identity. Always defined for an attributed write.
- **`$owner`** — the object's declared **`Owner` header** (`owner_ref` on update). *Not path-derived.*

And an undefined variable leaves the literal `$owner` token in the pattern, which matches no real path segment — **the pattern fails closed**.

| | 530 `{to: "*", on: "/$user/**"}` | 506 `{to: "owner", on: "/u/$owner/**"}` |
|---|---|---|
| Owner header | not needed | **required, or the write is rejected** |
| who may write | any signer, under their own canonical id | the actor must be authorized as the declared owner |
| ownership | implicit in the path | explicit in the object |

### The concrete work

**browserid-pay never sets an `Owner` header today.** `sdk/agreements/src/envelope.mjs:39` supports it (`if (m.owner) h.Owner = m.owner`) but `agreements.mjs` never passes one, and the custodian and judge write paths do not either — only `crates/chain/src/identity.rs:197`, for identity claims. Without one, every `/u/$owner/**` write fails closed.

**The blast radius is narrow.** Agreements live at `/agreements/**`, granted `{to: "*", can: ["create"]}` with no `$user`/`$owner` in the pattern — **unaffected by this choice**. The only thing in the user namespace is attestations.

- [ ] set `Owner` on every attestation write (custodian, judge)
- [ ] attestation path `/<issuer>/attestations/…` → `/u/<issuer>/attestations/…`; the path comes from the shared `sbo-core presets.rs: attestation()`, so it needs parameterising — coordinate with mingo, which also uses it
- [ ] update every reader of attestation paths: indexer (`index.rs`), the SDK, the judge's evidence reads
- [ ] drop the `/$user/**` grant from browserid-pay's genesis policy — it is not needed on 506

### Two properties worth keeping deliberately

- **Attestations stay stored under the issuer**, not the subject, so a subject still cannot delete claims about themselves. Preserved by the move; do not lose it.
- **An issuer *can* delete attestations it issued** — mingo's grant is `can: ["*"]` on `/u/$owner/**`. Not a regression (530 granted `post, delete` on `/$user/**`) and arguably correct as revocation, but it means the custodian could retract a scorecard. Deliberate, not inherited. `govern` is excluded from `*` by design, so policy itself is safe.

### Sharing `/u/**` across both apps is a feature

The issuers (`pay.dsp.fyi`, `judge.dsp.fyi`) are not mingo users, so there is little practical overlap. Where there *is* — an agent that both posts on the forum and has reputation — one identity having one namespace is exactly what makes the forum↔reputation link natural. That is what the single chain was for.

## Consequences to accept knowingly

- **Reputation resets to zero.** Every attestation and scorecard on 530 is abandoned. Fine for demo data, but reputation is the part of this project that is credible and shipped, so say it out loud rather than discover it.
- **Blast radius.** Money and a public forum now share a chain; a root-policy mistake touches both. The contribution floor already needed care to install; forum rules now need the same.
- **The contribution floor needs an upgraded daemon** (`sbo-pu34` object-id + payload-field conditions). An older validator **silently ignores** both fields and does not enforce the floor. Same daemon serves both apps, so this should be fine — **verify the deployed version rather than assume.**
- **Checkpointing comes for free.** 506 has a dedicated checkpointer key on `/sys/checkpoints/**`; 530 had none.

## What this unlocks

- **Write-time party checks become possible.** The one qualitative argument for merging: a policy on 506 can now see an agreement's parties, so "only a party may post an outcome reply" (`browserid-pay-ctnd`) stops being indexer-only.
- **The forum↔agreement backlink stops being cross-chain** (`browserid-pay-xnok`) — same chain, same block numbering.
- **One indexer sees discussion, agreements and attestations together**, which is what lets forum activity feed reputation at all.

- [ ] verify the deployed daemon enforces `sbo-pu34` field/id conditions **before** publishing the contribution floor
- [ ] governed root-policy update on 506 adding the grants and restrictions above
- [ ] adopt `/u/**` (decided) — see the namespace section above for the work
- [ ] re-post `/sys/contribution` and both provider listings
- [ ] repoint custodian, judge, indexer and the SDK/MCP config at 506
- [ ] reindex from 506 genesis
- [ ] decide what happens to 530 — left as history, or torn down
- [ ] update `GENESIS.md` and the design doc, which describe a two-chain world

## `/u/**` adoption done in code (2026-09-18)

The namespace half of this epic is implemented. **It was much smaller than estimated, because two assumptions were wrong:**

1. **browserid-pay does NOT use the shared sbo attestation preset.** The custodian and judge build the path themselves (`publish.rs`, `judge/src/main.rs`), so `mingo-ibwg` — parameterising `sbo-core presets.rs: attestation()` — is **not a blocker for this repo**. It still matters for mingo's own attestations if mingo ever wants a different prefix.
2. **The `Owner` header was already being set.** `ChainIdentity::sign_write` (`crates/chain/src/identity.rs`) passes `owner = agent.email()` on every write, and every Rust write goes through it. So `/u/$owner/**` already has the variable it needs — the earlier note in this bean that "browserid-pay never sets an Owner header today" was wrong, and the work it implied does not exist.

The JS SDK still sets no `Owner`, which is fine: it only writes under `/agreements/**`, whose grant has no `$owner` in the pattern.

**Changed:** attestation write paths (custodian, judge), the two warrant scopes `path:/*/attestations/**` → `path:/u/*/attestations/**`, the indexer's path parsing, the demo assertion, the spec and the judge README.

**The indexer accepts ONLY the `/u/` shape.** `issuer` is taken from the path (the payload carries `subject`, never the issuer), so accepting a bare `/<x>/attestations/…` would let anything written at the top level be read as an issuer. Guarded by a test.

## Still to do — operations, not code

- [ ] verify the deployed daemon enforces `sbo-pu34` field/id conditions **before** publishing the contribution floor (an older validator ignores them silently, so the floor would look installed and not be)
- [ ] governed root-policy update on 506: the two provider grants, `create` on `/agreements/**`, the providers/agreements size restrictions, and the contribution floor. 506 already has `/u/$owner/**`, `/sys/names/*`, the DNSSEC fragment and admin `govern` on `/**`, so nothing else is needed and there is no regenesis
- [ ] re-post `/sys/contribution` and both provider listings on 506
- [ ] repoint custodian, judge, indexer and the SDK/MCP repo config at 506
- [ ] reindex
- [ ] decide what happens to 530 — left as history, or torn down
- [ ] `crates/genesis` builds a WHOLE chain and is no longer the thing that runs; its root policy is now the reference for the governed update instead. Decide whether to keep it as a reference, repurpose it to emit the update, or delete it

## Status (2026-09-18) — code side complete; what remains is ops

The two checklists above were appended separately and overlap. This is the real state.

### Done

- [x] **`/u/**` namespace** — attestation write paths, both warrant scopes, the indexer's path parsing (new shape ONLY), demo, spec, judge README
- [x] **`Owner` header** — was already set: `ChainIdentity::sign_write` passes `owner = agent.email()` on every write. No work was needed; the earlier note here was wrong
- [x] **the shared sbo preset is not involved** — browserid-pay builds attestation paths itself, so `mingo-ibwg` does not block this repo
- [x] **repointed to 506** — `chain::DEFAULT_REPO` is now one definition; custodian, judge, indexer, SDK, MCP, demo and READMEs follow it
- [x] **`COLLECTION_AUDIENCE`** — the standing-consent audience was a hardcoded `530` const. Missing it would have refused **every standing grant**, silently: grants issued for one audience, checked against another. Now 506, with a test asserting it tracks `DEFAULT_REPO`
- [x] **`policy-layer` tooling** and a guard on `policy-update` — see below
- [x] **runbooks** — both `GENESIS.md` files

### The finding that changes who does the ops step

**The governed update on 506 must be signed by MINGO's sys key.** mingo's root
policy grants `govern` only to its own pubkey, so a policy write signed by
`dsp-sys.key` is refused — safe, but opaque.

And it is a **merge, not a replace**. `/sys/policies/root` on 506 is mingo's; writing browserid-pay's over it would remove mingo's community governance, its checkpointer grant and `/u/$owner/**`. `dsp-genesis policy-update` does precisely that wholesale replace, which was right for 530 and is wrong here — **it now refuses any repo that is not 530**, rather than relying on the daemon's authorization check to catch it.

New: `dsp-genesis policy-layer` prints just browserid-pay's grants and restrictions, as a fragment to merge.

### Remaining — all operational, none of it code

- [ ] **verify the deployed daemon enforces `sbo-pu34`** (object-id + payload-field conditions) **before** publishing the contribution floor. An older validator ignores both fields silently: the policy installs, looks right, enforces nothing
- [ ] merge `dsp-genesis policy-layer` output into 506's root policy; re-sign with **mingo's** sys key; submit
- [ ] `dsp-genesis policy-probe --repo …:506/` to confirm the floor actually bites — only once the daemon has synced past the policy block
- [ ] re-post `/sys/contribution` and both provider listings on 506
- [ ] deploy custodian, judge, indexer with the new default (or set the env vars)
- [ ] reindex
- [ ] run `demo/run-local.mjs` end to end — still the only thing that exercises the whole path, and unverified since the block-height work
- [ ] decide what happens to 530 — left as history, or torn down

### Open question, not blocking

- [ ] `crates/genesis` still builds a whole chain, which is now only meaningful for 530. `policy-layer` is the piece that stays useful. Keep the rest as a record, or strip it?
