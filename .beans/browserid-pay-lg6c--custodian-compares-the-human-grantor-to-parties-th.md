---
# browserid-pay-lg6c
title: Custodian compares the human (grantor) to parties that are agents (grantee) → not_payer live
status: completed
type: bug
priority: normal
created_at: 2026-09-15T20:48:13Z
updated_at: 2026-09-15T20:51:54Z
parent: browserid-pay-h1f5
---

SDK names agent identities as parties; escrow.rs compared caller.email (grantor). Fix: party checks use the grantee; payer/payee accounts are the grantors (payer from the lock presentation, payee from the acceptance bundle passed at lock).

## Summary of Changes

Custodian: party checks use the caller's grantee (acting identity); payer_account/payee_account (the grantors, payee's from the new required acceptance_bundle at lock) hold the money; adjudicator paid as named. Judge dispute intake checks the grantee. SDK fund() sends the acceptance bundle. demo-idp/demo now issue delegated presentations (alice+agent@… granted by alice@…), so local mode exercises the live shape.
