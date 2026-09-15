---
# browserid-pay-yph3
title: 'sbo-daemon: keep /sys/dnssec/<issuer> fresh automatically for issuers it sees'
status: in-progress
type: feature
priority: normal
created_at: 2026-09-15T16:20:10Z
updated_at: 2026-09-15T16:23:38Z
parent: browserid-pay-h1f5
---

Dan expected the daemon to submit a DNSSEC proof bundle ahead of a record that needs it. Today it does not: validate.rs only reads on-chain /sys/dnssec/<issuer> (deterministic replay), status.rs live-captures only for revocation-status keys, and /v1/dnssec merely reports + hands the caller a proof to submit. mingo runs a separate dnssec-refresh binary (and its 506 evidence lapsed 2026-09-02); browserid-pay has make dnssec-refresh (unscheduled, bean y1yd). Proposal: a daemon sweep that, for every issuer domain seen in attributed writes on a followed repo (plus a configured list), captures and submits a fresh dnssec.v1 before the RRSIG window lapses — needs per-repo TurboDA credentials (bean lyir). Writers that submit directly to TurboDA (judge, SDK) never pass /v1/submit, so a submit-time hook alone would not cover them. Work lives in ~/src/sbo.

Correction 2026-09-15: mingo DOES do on-demand refresh at write time, but client-side (mingo-app login.rs presentation_issuers + seed.rs ensure_dnssec_fresh; mingo-idp poster.rs ensures every distinct issuer). Dan: generalize it into the daemon's /v1/submit so no writer re-implements it. Fork 'daemon-evidence' implementing in ~/src/sbo + mingo deploy; lyir (per-repo TurboDA) folded in.
