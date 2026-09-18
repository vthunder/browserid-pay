---
# browserid-pay-031y
title: Decide the migration discriminator for height-based deadlines
status: todo
type: task
priority: high
created_at: 2026-09-18T13:38:58Z
updated_at: 2026-09-18T15:07:43Z
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
