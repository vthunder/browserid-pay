---
# browserid-pay-031y
title: Decide the migration discriminator for height-based deadlines
status: completed
type: task
priority: high
created_at: 2026-09-18T13:38:58Z
updated_at: 2026-09-18T16:12:59Z
blocked_by:
    - browserid-pay-4v59
---

The most dangerous part of the block-height change, and it must be settled **before** any deadline is written in the new units.

A height and a unix timestamp are both integers. An old `deadline: 1760000000` read as a height is a block number in the distant past, so **every obligation is instantly late and nothing errors**. There is no type difference to catch it.

Live agreements on 530 carry unix deadlines today.

- [ ] version discriminator: bump `v` on the terms, or a distinct `deadline_block` field alongside the old one?
- [ ] what a new client does when it reads an old agreement — refuse, or evaluate in the old units?
- [ ] old and new `deriveObligations` cannot interoperate, so both parties must move together: coordinated cutover, or versioned evaluation
- [ ] the live agreements on 530 — migrate, or let them run out under old rules?

## Resolution (2026-09-18)

All four questions are answered, and the answer to the last one collapses the rest.

- **Discriminator: rename the field.** `deadline` → `deadline_block`, `expires` → `expires_block`. A version integer only protects a reader that checks it; a rename fails loudly in both directions — an old client finds no `deadline` and hits the existing "everything needs a bound" error, a new client seeing `deadline` knows it is old units. No silent misreading is possible, which matters because both values are bare integers.
- **What a new client does with an old agreement:** the question does not arise. All app data is being wiped (`browserid-pay-4v59`).
- **Interop:** none to build. Hard cutover.
- **Live agreements on 530:** abandoned along with the chain.

The same principle was already applied in the judge (`browserid-pay-dtjt`), where `respond_by` → `respond_by_block` was renamed in the struct, the SQL column, the env var and the API field — every boundary the value crosses.

## Summary of Changes

A decision, no code. Implementation of the rename is part of the SDK conversion.
