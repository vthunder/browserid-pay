---
# browserid-pay-ax7h
title: 'Agent permissions: checklist and one-pass setup'
status: todo
type: epic
created_at: 2026-09-19T21:42:22Z
updated_at: 2026-09-19T21:42:22Z
---

See docs/plans/2026-09-19-agent-permissions-setup.md.

- [ ] Drop `"*"` from the counterparties description; omit = anyone
- [ ] Bundle extraGrants (the relay) in requestStandingConsent
- [ ] max_per_agreement parameter, enforced at lock (spec change)
- [ ] Payee card wording, or enforce the cap at accept
- [ ] A `setup` playbook: one conversation, one card
