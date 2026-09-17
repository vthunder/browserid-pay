---
# browserid-pay-3a8p
title: On-chain bundles carry a 5-minute assertion, so async agreements cannot work
status: completed
type: bug
priority: critical
created_at: 2026-09-17T15:44:04Z
updated_at: 2026-09-17T16:11:30Z
---

Found in a live run 2026-09-17, after the standing-audience fix unblocked bob.

A proposal embeds the proposer's full presentation as `bundle`. Decoding the one in `agr_fUJ-HR0o5vRu8udh`:

    access-cert  exp=1789745611  ok      (24h)
    assertion    exp=1789659511  EXPIRED (5 MINUTES)
    warrant      exp=1797435191  ok      (90d)
    config-cert  exp=1796915071  ok

The **assertion lives five minutes**. The acceptor verifies that bundle whenever it gets round to accepting, so an agreement can only be accepted within five minutes of being proposed. After that: `bad_bundle: Invalid provisioning material: assertion: expired`.

## It is systemic, not one call site

Every embedded bundle read off the chain later has the same window:

1. **propose → accept** — the acceptor verifies `p.bundle` (`#checkBundle`).
2. **accept → fund** — `fund` reads the acceptance bundle back off chain (`acc?.payload?.bundle`) and hands it to the custodian, which verifies it. So even after accepting in time, funding late fails the same way.
3. Same for `payeeIsProposer ? p.bundle`.

## Why it never showed before

Every prior run was human-driven and interactive — each step happened within minutes. The moment agents run on a 60s loop with approval waits in between, gaps routinely exceed five minutes. **This is the blocker for autonomous operation**, not a corner case: it makes the async design unusable exactly when the agents start behaving autonomously.

## The tension

An on-chain artifact must prove the proposer held authority, and stay verifiable later. A presentation's assertion is deliberately short-lived to stop replay — and publishing it on chain makes it world-readable, so lengthening it publishes a replayable bearer credential. Both cannot be true of the same object.

## Options (undecided)

1. **Publish authority, not possession.** Carry the warrant + certs; drop the live assertion. The chain object is already signed and attributed to the proposer by SBO, so possession of the identity is proven by the write itself — the embedded assertion is arguably redundant with it. Needs the verifier to check a warrant without requiring a fresh assertion.
2. **Verify as of the object's block time.** The chain is a trusted clock. `@browserid-ng/verify` has no such option today (`VerifyCallOptions` is only `acceptedFallbacks`), so this needs a change there.
3. **Bind the assertion to the object** — nonce = the proposal hash — so a long-lived assertion cannot be replayed for anything else. Cleanest cryptographically, biggest change.
4. **Long-lived assertion.** Rejected: publishes a replayable credential.

Likely needs a change in browserid-ng (`~/src/browserid-ng`), not just here.

## Immediate workaround

Re-propose and accept within five minutes. Nothing else works today.

- [x] decide between (1), (2), (3)
- [x] fix propose → accept
- [x] fix accept → fund, and the custodian side
- [x] test with a deliberate delay longer than the assertion lifetime — the gap that no current test covers

## Summary of Changes

Took option 1: **publish authority, not possession** (Dan, 2026-09-17 — "sbo already guarantees that an object posted by an email can only be posted by that email or someone authorized to act as that email").

No proposal or acceptance carries a presentation any more. Who wrote each object is settled by SBO attribution, which the daemon enforces at write time, so the embedded credential was redundant for identity and fatal for durability.

**Custodian — the ledger is what it verifies against.** `lock` now reads the proposal AND acceptance back off the chain through the `ChainReader` it already had, instead of trusting what the caller handed it, and checks each object's `creator` against the party it names. A forged proposal or an acceptance written by someone who is not the named acceptor is refused as `bad_attribution`. The proposal-hash check went away with it: a `(path, id)` slot holds exactly one first-valid write, so there is no other version to have referenced.

**Consequences that had to be solved, not assumed:**

- *The payee's account.* Attribution names the accepting IDENTITY; the money belongs to the human behind it. New `POST /api/escrow/register`: an acceptor presents a contract grant once and the custodian keeps the identity → account mapping. Deliberately not a field on the acceptance object — the grant's grantor is signed by the human, so an agent cannot redirect its own human's earnings, which a self-declared account would allow.
- *The acceptor's own `min_reputation`.* It used to ride in their grant. It now travels on their acceptance object, which is attributed to them, so it is still their signed statement.
- *Reputation aggregation.* The indexer read each party's controlling account out of the embedded bundles; without them **nothing would ever have counted** (`why_not_counted: "a party's controlling account is unknown"`), silently disabling the same-account sybil guard. The custodian now writes both accounts onto the lock object it already posts — attested, since it verified both — and the indexer prefers that, falling back to the old bundles so existing agreements keep working.
- *The acceptor's early checks.* Cap and counterparty pre-flight are gone: there is no credential to inspect, and they were never the enforcement point (the custodian refuses the lock). `min_reputation` is now declared in the clear on the proposal so an acceptor can still tell before doing work whether it qualifies.

**Demo.** Two latent problems surfaced once the custodian started reading and writing: it was pointed at the PRODUCTION daemon while the demo wrote to a stub, and it had no chain identity at all, so every attestation and lock object it "posted" was silently dropped. Both fixed in `run-local.mjs`.

**Tests**: acceptance long after the proposal (the gap nothing covered); forged attribution on the proposal and on the acceptance; no acceptance on chain; plus the existing 25 SDK and 21 custodian tests, and the local e2e.
