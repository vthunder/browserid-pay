---
# browserid-pay-rhqt
title: 'mingo: attestation path prefix needs to be configurable (or move to /u/)'
status: todo
type: task
priority: low
created_at: 2026-09-18T19:25:26Z
updated_at: 2026-09-18T19:25:26Z
---

Tracked here, not in mingo — see the sibling bean and `deploy/GENESIS.md` → "Keep this out of the shared repo".

browserid-pay is moving onto chain 506 alongside mingo (`browserid-pay-4v59`) and adopting mingo's `/u/**` user namespace, so there is one convention on the chain rather than two.

That requires the shared attestation path to become configurable. `sbo-core presets.rs: attestation()` builds the object at `/<issuer>/attestations/<subject>/` with `ID: <type>` — a hardcoded top-level namespace. Under 506's root policy the grant is `{ to: "owner", can: ["*"], on: "/u/$owner/**" }`, so attestations need to land at `/u/<issuer>/attestations/<subject>/`.

**Why this touches mingo:** the preset is shared, so whatever it does affects both. Either it grows a namespace prefix parameter, or mingo and browserid-pay agree on `/u/` and the preset is simply changed — mingo's own attestations (membership, ban, endorsements) would move with it.

Worth checking what mingo writes through this preset today and where those objects currently land, since the answer decides whether this is a parameter or a straight change.

- [ ] what does mingo attest through `presets.rs: attestation()`, and at what paths today?
- [ ] parameterise the prefix, or change it to `/u/` for everyone?
- [ ] if changed: mingo's existing attestations on 506 are at the old paths — do they need rewriting, or does the policy grant both for a period?
- [ ] coordinate the cutover with `browserid-pay-4v59`
