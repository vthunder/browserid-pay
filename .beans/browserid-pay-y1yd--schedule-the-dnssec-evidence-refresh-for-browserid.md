---
# browserid-pay-y1yd
title: Schedule the DNSSEC evidence refresh for browserid.me on app 530
status: todo
type: task
priority: normal
created_at: 2026-09-15T15:25:14Z
updated_at: 2026-09-15T15:25:15Z
parent: browserid-pay-h1f5
---

`make dnssec-refresh` works but nothing runs it; RRSIG windows are days–weeks, and mingo learned that expired evidence silently breaks broker-attributed writes (mingo 506 has been expired since 2026-09-02). Options: a GitHub Actions schedule in browserid-pay (needs the TurboDA key as a repo secret) or a cron on the hobby host.
