# browserid-pay genesis record — v1 (2026-09-15)

> **SUPERSEDED IN DIRECTION (2026-09-18).** browserid-pay no longer has a chain
> of its own. App **530 is being abandoned** and agreements move onto **app 506,
> alongside mingo** (`browserid-pay-4v59`). Everything below documents the v1
> standalone chain and stays accurate *as a record of what was built and why* —
> the policy section in particular is the reference for what must now be applied
> **on top of mingo**. Start with **"Layering on mingo"** below.

The browserid-pay SBO database: agreements, provider listings, attestations
(design: `docs/plans/2026-09-15-agreements-design-draft.md`, "The chain").
Tooling: `crates/genesis` (`dsp-genesis`), Makefile `genesis-*` targets. Modelled
on mingo's `deploy/GENESIS.md`; read that one for the war stories.

---

## Layering on mingo

browserid-pay is now a **layer on mingo's chain**, not a chain. There is no
browserid-pay genesis any more: there is a set of grants, restrictions and
objects that must exist on top of whatever mingo last genesised.

**The dependency runs both ways, and that is the whole point of this section.**
mingo's `deploy/GENESIS.md` carries a matching warning, because a mingo
regenesis silently destroys this layer — the grants below are not in
`mingo-app/src/genesis.rs` and will not be reborn with the new chain.

### What mingo must provide (do not let a regenesis drop these)

| | why browserid-pay needs it |
|---|---|
| `owner: *` on `/u/$owner/**` | attestations live at `/u/<issuer>/attestations/<subject>/`. Note `$owner` is the **`Owner` header**, not the path — an absent Owner leaves the pattern unsubstituted and **fails closed**. `ChainIdentity::sign_write` sets it on every write |
| `/sys/names/*` create + owner-update | identity name claims |
| the DNSSEC self-auth fragment | keeps issuer evidence fresh on our own writes |
| `browserid.me` as trust broker | attribution for `@sandmill.org` / `@dsp.fyi` identities |
| admin `govern` on `/**` (sys key) | without it none of the below can be installed |

### What browserid-pay adds, by governed update from the sys key

No regenesis is needed for any of it — the sys key holds `govern` on `/**`.

**Grants**
```
*   post     /providers/custodians/$user/**
*   post     /providers/adjudicators/$user/**
*   create   /agreements/**
```

**Restrictions**
```
/providers/**    require schema provider.v1, max_size 16384
/agreements/**   require max_size 65536
/agreements/*/  id=proposal  require fields:
    /contribution/fn          eq   "max"
    /contribution/amount      min  MIN_CONTRIBUTION_AMOUNT
    /contribution/percentage  min  MIN_CONTRIBUTION_PERCENTAGE
```

**Objects**
- `/sys/contribution` (what agents should offer + the floor actually enforced)
- the custodian and adjudicator provider listings

The exact shapes are in `crates/genesis/src/main.rs` — see **Policy** below,
which remains the authoritative expression even though the crate no longer
builds the chain that runs.

### Who can perform it — read this first

**Chain-level changes need the SHARED CHAIN's admin key**, which lives at
`~/secure-backup/mingo-sys.key`. The file name and its `sys@mingo.place`
identity are historical: since 506 became a shared base chain it is
infrastructure, not mingo's application key. `dsp-sys.key` holds no `govern`
there, so a policy write signed by it is refused — safe, but opaque.

Practically, two keys with different jobs:

| key | what it signs |
|---|---|
| `mingo-sys.key` (chain admin) | the root policy, and anything under `/sys/**` |
| `dsp-sys.key` | nothing on 506 today. It remains browserid-pay's own identity and signed 530 |

Everything else — agreements, provider listings, attestations — is written by
the **service identities** under warrants, not by either sys key.

**`/sys/contribution` needs the admin key.** It is browserid-pay's config living
in the chain's `/sys/` namespace, and nothing grants an application write access
there. Deliberately left as-is (Dan, 2026-09-18): it is read by path and no
reader checks the creator, so the only cost is the key you sign with.

```
dsp-genesis contribution-update --repo sbo+raw://avail:turing:506/ \
    --key-file ~/secure-backup/mingo-sys.key
```

**Later, not now:** the clean separation is three repos — base chain, mingo,
browserid-pay — so the chain's own concerns stop living inside an application.

And the update is a **merge, not a replace**. `/sys/policies/root` on 506 is
mingo's policy; writing browserid-pay's over it would remove mingo's community
governance, its checkpointer grant and `/u/$owner/**`. `dsp-genesis
policy-update` does exactly that wholesale replace — correct when browserid-pay
owned 530, wrong here — so **it now refuses any repo that is not 530**.

Use instead:

```
dsp-genesis policy-layer        # prints just browserid-pay's grants + restrictions
```

Merge that fragment into 506's root policy and re-sign the whole thing with
mingo's sys key.

### Strings that must move together (a regenesis hazard)

If a regenesis ever changes the repo URI — a new app id, or a different URI
form — these must all change with it. They are compared against each other and
**a mismatch is silent**:

| | |
|---|---|
| `chain::DEFAULT_REPO` (`crates/chain/src/lib.rs`) | the one definition every service defaults to |
| `COLLECTION_AUDIENCE` (`crates/custodian/src/escrow.rs`) | `<repo>agreements` — the **standing-consent warrant audience** |
| `collectionAudience()` / `DEFAULTS.repo` (`sdk/agreements`) | the SDK's computed form of the same string |
| the daemon's `repos.json` entry | `mingo/deploy/sbo-daemon/entrypoint.sh` |

**Why it is silent:** a standing grant is *issued* for the SDK's audience and
*checked* against the custodian's const. If they differ by one character, no
grant ever matches and **every standing lock is refused** — with no error that
says why, because an audience miss is indistinguishable from "no such grant".
This is deliberately not a revocation path, so nothing logs it as unusual.

Guarded by `escrow::tests::the_collection_audience_tracks_the_repo`, which
asserts the const equals `format!("{}agreements", chain::DEFAULT_REPO)`. The
SDK side has no such test — it computes the string, so it cannot drift on its
own, but its `DEFAULTS.repo` still has to be edited by hand.

Also note the repo string **is the warrant audience**, so changing it
invalidates consent already granted. That is correct — a human consented to
spending on a specific ledger — but it means every standing grant must be
re-approved after a repoint, and the humans should be told.

### Order, and the one that bites

1. **Verify the daemon first.** The contribution floor needs the `sbo-pu34`
   object-id and payload-field conditions. **A daemon without them ignores both
   fields silently**: the policy installs, looks right, and enforces nothing.
   Check the deployed rev before publishing it, not after.
2. Install grants and restrictions.
3. Post `/sys/contribution` and the provider listings.
4. Repoint the custodian, judge, indexer and SDK/MCP config at the repo.
5. Reindex from genesis.
6. Re-run `demo/run-local.mjs` end to end — it is the only thing that exercises
   the whole path.

### Redeploying after a repoint — one consent each

Changing the repo changes the **warrant audience**, and a stored credential for
the old ledger still LOADS. Before this was handled, a redeployed service came
up healthy and then failed every write, because an audience miss reads as "no
matching grant" rather than "wrong ledger" — a silent outage on a service that
holds money.

Both services now compare the loaded credential's warranted audiences against
`chain_audience` and re-provision when it does not hold
(`Identity::has_audience`). So the repoint costs **one consent approval per
service**, at the URL each prints on startup and exposes at `/api/setup`. That
cannot be automated; it is the point of the consent card.

Provider listings need no step: `ensure_listing` runs at startup and the
`/providers/<kind>/$user/**` grants are in place.

### What does NOT carry over

**Reputation resets.** Every attestation and scorecard is gone. Accepted
deliberately (Dan, 2026-09-18) — it is test data — but say it out loud, because
reputation is the part of this project that is shipped and credible.

---

**v1 canonical identity:** `avail:turing:530:3885219:sha256:6e36ddae51fcf93557d6be930cda3849ededbdf226241af08f6f7f07e7e86d51`

| Field | Value |
|---|---|
| Chain | `avail:turing` (Avail turing testnet) |
| App ID | `530` |
| **Genesis block (B)** | **3885219** |
| **Genesis hash** | `sha256:6e36ddae51fcf93557d6be930cda3849ededbdf226241af08f6f7f07e7e86d51` |
| Mode | **A** — self-signed `sys`, no domain object (see below) |
| Pinned broker | `browserid.me` (fallback attribution for every identity here) |
| sys pubkey | `ed25519:3464f4e59cfb240f246618094a41c42ec0dbd4ea73f5cf645999c7938fcc62e4` |
| TurboDA submission | `39020467-6852-427c-bd75-905d60269b01` (tx `0x2c6e8b85…`, extrinsic 3, block hash `0x77c97124…`) |
| Genesis wire | `genesis.wire` (6805 bytes, 5 messages, committed alongside this file) |
| Daemon | `da.sandmill.org` (the mingo-deployed `sbo-daemon`, following 506 **and** 530) |

## DNS record (`_sbo.dsp.fyi`) — for Dan to set at Namecheap

```
_sbo.dsp.fyi.  IN  TXT  "v=sbo1 repo=sbo+raw://avail:turing:530@3885219/ genesis=sha256:6e36ddae51fcf93557d6be930cda3849ededbdf226241af08f6f7f07e7e86d51 node=https://da.sandmill.org"
```

`@3885219` is the genesis anchor; `genesis=` the identity hash; `node=` the
`/v1/*` read RPC. Discovery only — never a trust root (Authorization spec: a
warrant audience is always the `sbo+raw://` form).

## What the genesis contains (in block order)

1. `/sys/dnssec/browserid.me` — `dnssec.v1`, the RFC 9102 proof for
   `_browserid.browserid.me` captured at authoring time through the daemon's
   `/v1/dnssec`. Sys-signed, but self-authenticating (any signer may refresh it
   — see the policy). Present at genesis so broker-attributed writes work from
   block one instead of failing L2 attribution (mingo's first-week lesson).
2. `/sys/names/sys` — self-signed `identity.v1` (Mode A).
3. `/sys/trust/brokers` — `["browserid.me"]`, the on-chain attribution anchor.
4. `/sys/contribution` — `contribution.v1`
   `{"recipient":"fund@dsp.fyi","suggested_percentage":1.0}`. A *suggestion*
   SDKs read at discovery. The chain-enforced minimum from the design is NOT
   here: it needs a payload-field policy restriction the Policy spec lacks
   (bean `browserid-pay-er8j`); adding it later is a governance policy update.
5. `/sys/policies/root` — `policy.v2`, written last so 1–4 are admitted in
   genesis mode.

### Why Mode A (no `/sys/domains/dsp.fyi`)

A `domain.v1` does two things: certify T1 names (`*@<repo-domain>`, issued by
a browserid primary the repo runs) and self-certify a repository root of trust
(Identity spec §Domain Objects, Genesis spec §Modes). dsp.fyi runs no primary —
`escrow@dsp.fyi`, `judge@dsp.fyi`, `fund@dsp.fyi`, and every human and agent
here are T0 emails attributed via the pinned broker — so a domain object would
certify nothing. If dsp.fyi ever publishes a `_browserid` record (hosted primary
at browserid.me, one DNSSEC TXT), that is a **regenesis** to Mode B, not an
update.

### Policy

```json
roles:  admin = { key: <sys pubkey> }          // the KEY, never a name (mingo-d7bi)
grants:
  *       create            /sys/names/*         // first-come name claims
  owner   update, delete    /sys/names/*
  *       post, delete      /$user/**            // v1 only. On 506 this is mingo's
                                                 // `owner: * on /u/$owner/**`, and
                                                 // attestations moved with it to
                                                 //   /u/<issuer>/attestations/<subject>/<type>
  *       post              /providers/custodians/$user/**   // exactly its own listing:
  *       post              /providers/adjudicators/$user/** //   /providers/<kind>/<identity>/listing
                                                 // (patterns match PATHS, not ids — v1 genesis had
                                                 //   `/providers/<kind>/$user`, which could never match;
                                                 //   fixed by a governed policy update, 2026-09-15)
  *       create            /agreements/**       // append-only; see the gap below
  *       create, update    /sys/dnssec/**       // self-authorizing (shared sbo fragment)
  admin   post, transfer, delete, govern   /**   // governance
restrictions:
  /providers/**    schema provider.v1, max_size 16 KiB
  /agreements/**   max_size 64 KiB
  /agreements/*/   id `proposal`: contribution floor (see below)
  /sys/dnssec/**   schema dnssec.v1 + dnssec_proof (the shared fragment)
```

#### The contribution floor

Every proposal must contribute at least `max(1 cent, 1bp)` to the ecosystem
recipient. Deliberately tiny: it exists so that contributing *nothing* is not
expressible, not to price the ecosystem. The **suggested** contribution, in
`/sys/contribution`, is 0.5%.

```json
{"on": "/agreements/*/", "id": "proposal", "require": {"fields": [
  {"pointer": "/contribution/fn", "eq": "max"},
  {"pointer": "/contribution/amount", "min": 0.01},
  {"pointer": "/contribution/percentage", "min": 0.01}
]}}
```

Three things make this work, each load-bearing:

- **`id: proposal`.** An agreement's objects share one container
  (`/agreements/<id>/` with ids `proposal`, `acceptance`, `lock`, `ruling`,
  evidence). Without the id the requirement would apply to all of them, and
  every acceptance would be rejected for carrying no contribution.
- **Pinning `fn`.** Contribution is a *formula* on chain —
  `{recipient, fn, amount, percentage}`. Without `fn == "max"` a proposer picks
  `min` and pays the smaller operand.
- **Flooring both operands.** Conservative for a floor on `max()` — stricter
  than strictly necessary, and sound. The chain cannot do better: the exact
  arithmetic needs the *price*, which is sealed in commitment mode. **The
  custodian does that arithmetic at lock**, so enforcement is two-layered.

**Requires daemon support.** Object-id matching and payload-field conditions
arrived in sbo `1eb6d21` (bean `sbo-pu34`). A daemon older than that **ignores
both fields** and silently does not enforce this. Upgrade every validator
*before* publishing the policy — mixed versions would disagree about which
writes are valid. The daemon's rev is `SBO_REV` in mingo
`deploy/sbo-daemon/Dockerfile`.

Publish a policy change with `make policy-update` (`--dry-run` first); the sys
key holds `govern` on `/**`, so this never needs a regenesis. Republish
`/sys/contribution` separately with `dsp-genesis contribution-update` — the two
objects are independent, and changing the floor without the suggestion leaves
them inconsistent.

**A published policy is not an enforced policy until the daemon has synced to
it.** `policy-update` submits through the daemon's queue (a batch to TurboDA,
finalized a minute or two later), and the daemon validates against its *own
confirmed state* plus the mempool. Between leaving the mempool and being
replayed from the chain, the new policy is in neither, so the OLD one governs.
After a redeploy the daemon replays from its last checkpoint and can be
thousands of blocks behind — watch `Processed block N` in
`dokku logs sbo-daemon` and wait for it to pass the block `policy-update`
reported.

Verify enforcement with `dsp-genesis policy-probe`, which submits a proposal
that breaches the floor and expects a refusal. A refused write never lands, so
a passing probe leaves nothing behind — but a probe run against a lagging
daemon reports a false failure *and* leaves a junk proposal on chain.

**The gap, stated plainly.** The design says "evidence by the two parties,
`ruling` by the named adjudicator, `lock` by the named custodian". The policy
language cannot read the proposal object to learn who those are, so on chain
`/agreements/**` is merely *append-only by any attributed identity*: nothing
under an agreement can be updated (first valid write at a `(path,id)` slot is
final), and every object is signed and attributed. **Who may accept, rule, or
lock is enforced by consumers** — the custodian, the indexer, the SDK — by
checking each object's attributed author against the parties the proposal
names, exactly as attestation *weight* is a consumer judgment in the
Attestation spec. A stranger's "acceptance" is valid paper nobody honours. If
the Policy spec later gains cross-object variables, this tightens by a
governance policy update, no regenesis.

## Key backup (DO NOT LOSE)

- sys key → `~/secure-backup/dsp-sys.key` (`{"secret_key":"<hex seed>"}`, mode
  600; `dsp-genesis keygen` refuses to overwrite). Copy it to the password
  manager / an offline medium the same day. The sys key is the only governance
  authority (`govern` on `/**`); losing it means a regenesis, exactly what
  happened to mingo v1.
- No domain key, no checkpointer key (this database does not publish
  checkpoints; the daemon's checkpointer is mingo's, keyed to 506).

## Daemon: one node, two databases

`da.sandmill.org` is the daemon mingo deploys (`mingo/deploy/sbo-daemon/`).
The daemon syncs every app id in its `repos.json`, so it indexes 530 next to
506 with **no second instance**. The durable change is in mingo's
`deploy/sbo-daemon/entrypoint.sh` (commit `864229a`, on mingo's main but **not
pushed** at the time of writing): on container start it appends the 530 entry to
`/data/repos.json` if absent (idempotent, keyed on repo id `ec2dbcf61eea9945`).
Pushing that commit triggers mingo's `deploy-daemon` workflow; after the
release the node backfills 530 from B and logs `Genesis verified` (or
`GENESIS VERIFICATION FAILED`). Then `make genesis-verify`.

Reads for 530 pass `repo=sbo+raw://avail:turing:530/` to every `/v1/*` call
(the daemon defaults to "the sole repo", which will no longer be sole).

**Writes** for 530 do **not** go through the daemon: its `[turbo_da]` block is
single-app (506). Every 530 writer — custodian, adjudicator, SDK, this tool —
submits to TurboDA directly with the app-scoped key (`.env` →
`AVAIL_TURBODA_KEY`). Bean `browserid-pay-lyir` tracks making the daemon per-repo.

Known wart to watch: `sync.rs` takes "the first repo" as representative for the
pre-state root of a block. State DBs are per repo, but the checkpointer /
prover paths were written for one database; if mingo's checkpoints or
snapshots misbehave after 530 is added, that is where to look.

## DNSSEC evidence upkeep (REQUIRED)

`/sys/dnssec/browserid.me` carries RRSIG windows of days to weeks. Attribution
of every write here is the intersection of the cert window and that proof
window, so the proof must be refreshed before it lapses or every write starts
failing L2 attribution. `make dnssec-refresh` asks the daemon whether the
530 copy is fresh (`/v1/dnssec?repo=…`) and, if not, captures, signs with a
throwaway key (self-authorizing object) and submits via TurboDA. Run it from
cron (daily is plenty; it is a no-op when fresh). Note: at the time of writing
the daemon reported mingo's own `/sys/dnssec/browserid.me` on 506 as
**expired** (2026-09-02) — mingo's refresher, not ours, but worth telling Dan.

## Reproduce / regenesis

> **This recipe builds the standalone v1 chain on app 530, which is being
> abandoned.** Kept because it documents how the pieces fit and how the daemon
> is told to follow a repo. For the current world, "regenesis" means *mingo*
> regenesises and this layer is re-applied — see **"Layering on mingo"** at the
> top, and `mingo/deploy/GENESIS.md`.


```sh
# 0. secrets + key
set -a; . ./.env; set +a                 # AVAIL_NETWORK=turing AVAIL_APP_ID=530 AVAIL_TURBODA_KEY=…
make build-genesis
~/.cache/cargo-target/debug/dsp-genesis keygen      # once; refuses to overwrite

# 1. build (captures a fresh browserid.me proof through the daemon)
make genesis-build                        # → genesis.wire + genesis hash

# 2. submit, then poll until "Finalized" (≈30–60 s)
make genesis-submit                       # → submission_id
make genesis-status ID=<submission_id>    # → data.block_number = B

# 3. record identity, DNS record, daemon entry
make genesis-identity BLOCK=<B>

# 4. make the daemon follow it: paste the printed repos.json entry into mingo
#    deploy/sbo-daemon/entrypoint.sh (DSP_ENTRY + DSP_REPO_ID), commit, push →
#    deploy-daemon runs. For a REGENESIS also drop the old 530 state on the
#    host first: the entrypoint's grep is keyed on the repo id, which does not
#    change across regenesis, so bump first_block/head/expected_genesis in the
#    entry AND `rm -rf /data/.sbo/repos/avail_turing_530 /data/repos/dsp` +
#    remove the old entry from /data/repos.json (dokku enter sbo-daemon web).
# 5. update this file (table + DNS record) and ask Dan to update _sbo.dsp.fyi.
# 6. every consumer that pins the genesis (custodian, SDK discovery) gets the new
#    canonical identity; the old chain stays on app 530 below B and is invisible.
```

The old (pre-regenesis) chain is never deleted — Avail is append-only — it is
simply below the new anchor. A regenesis with a *new* sys key: move
`~/secure-backup/dsp-sys.key` aside first (`keygen` refuses to overwrite).

## History

- **v1 2026-09-15 @3885219** (`6e36ddae…`) — first genesis. Mode A, broker
  browserid.me, contribution suggestion 1% to fund@dsp.fyi.
- **policy update 2026-09-16** — contribution floor `max(1 cent, 1bp)` on
  `/agreements/*/` id `proposal`; suggestion 1% → 0.5%. Governed update, no
  regenesis. Needed the daemon on sbo ≥ `1eb6d21`.
