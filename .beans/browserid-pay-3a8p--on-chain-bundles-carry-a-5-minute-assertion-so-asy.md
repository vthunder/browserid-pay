---
# browserid-pay-3a8p
title: On-chain bundles carry a 5-minute assertion, so async agreements cannot work
status: todo
type: bug
priority: critical
created_at: 2026-09-17T15:44:04Z
updated_at: 2026-09-17T15:44:04Z
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

- [ ] decide between (1), (2), (3)
- [ ] fix propose → accept
- [ ] fix accept → fund, and the custodian side
- [ ] test with a deliberate delay longer than the assertion lifetime — the gap that no current test covers
