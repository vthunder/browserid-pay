---
# browserid-pay-y1yd
title: Schedule the DNSSEC evidence refresh for browserid.me on app 530
status: scrapped
type: task
priority: normal
created_at: 2026-09-15T15:25:14Z
updated_at: 2026-09-16T16:25:18Z
parent: browserid-pay-h1f5
---

`make dnssec-refresh` works but nothing runs it; RRSIG windows are days–weeks, and mingo learned that expired evidence silently breaks broker-attributed writes (mingo 506 has been expired since 2026-09-02). Options: a GitHub Actions schedule in browserid-pay (needs the TurboDA key as a repo secret) or a cron on the hobby host.

## Reasons for Scrapping

The premise is wrong: no regular refresh is wanted. A DNSSEC proof should be written to the chain **on demand, only when a write needs one** — which the daemon already does, submitting the refreshed proof as part of accepting the write and reporting it on `/v1/submit` (`evidence_refreshed`, consumed at `sdk/agreements/src/chain.mjs:50`). Already built and deployed, so neither option this bean weighed (a GitHub Actions schedule, a host cron) is needed, and neither would have been better — both add key custody and a refresh that runs when nothing is writing.

Verified 2026-09-16: the 530 proof is fresh (2026-09-15 → 2026-09-24, `needs_refresh: false`). `make dnssec-refresh` remains as a manual escape hatch. Closed out under `browserid-pay-vpe8`.
