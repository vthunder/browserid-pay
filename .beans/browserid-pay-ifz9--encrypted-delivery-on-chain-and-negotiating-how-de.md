---
# browserid-pay-ifz9
title: Encrypted delivery on chain, and negotiating how delivery happens
status: draft
type: feature
priority: normal
created_at: 2026-09-17T17:31:10Z
updated_at: 2026-09-17T17:31:10Z
---

The structural fix for `browserid-pay-oz63`: put the deliverable itself on chain, sealed to the recipient, so availability is guaranteed by the ledger rather than asserted by the sender. The judge then sees exactly what the recipient could see, and "I sent it / they are lying" stops being possible. `seal()` / X25519 already exists for sealing terms to the adjudicator.

## Delivery mode should be a term, not a global rule

Agreements differ: a haiku can go on chain, a video cannot. So HOW delivery happens is something the parties agree, and the terms should say which regime applies — the guarantees are genuinely different and neither side should have to infer which one they are under.

Modes to support: `commitment` (today: hash on chain, content off-chain), `sealed` (ciphertext on chain, readable by the recipient), `public` (content in the clear, which already exists at the proposal level).

## How the encryption key gets agreed (Dan, 2026-09-17)

Three candidates, not yet chosen:

1. **An SBO-native mechanism** for posting content encrypted to an email identity — the most reusable, and the most work; it would serve anything on the ledger, not just agreements.
2. **A public key named in the off-chain proposal terms.** Private, simple, but invisible to anyone verifying later.
3. **A public key on the on-chain proposal.** Checkable by the adjudicator and anyone else, at the cost of publishing a key.

An agent directory entry would carry an encryption key anyway (`browserid-pay-h4mu`), which may make (3) nearly free.

## Costs to be explicit about

- The ciphertext is permanent and public: a future key compromise exposes the content forever. Already true of sealed terms to the adjudicator, but the blast radius is larger here.
- `/agreements/**` is capped at 64 KiB by the root policy. Fine for text, useless for media. Commitment mode has to stay for anything larger, which is why the mode has to be a term.

- [ ] pick the key-agreement mechanism
- [ ] `delivery_mode` in the terms, with the obligations model knowing what "met" means in each
- [ ] what the adjudicator does when the mode was `sealed` and the ciphertext does not decrypt
- [ ] size: refuse at propose time when the deliverable cannot fit the mode the terms claim
