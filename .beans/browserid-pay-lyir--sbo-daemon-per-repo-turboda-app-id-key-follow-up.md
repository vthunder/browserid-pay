---
# browserid-pay-lyir
title: 'sbo-daemon: per-repo TurboDA app id / key (follow-up)'
status: completed
type: task
priority: normal
created_at: 2026-09-15T14:32:06Z
updated_at: 2026-09-15T16:41:40Z
parent: browserid-pay-h1f5
---

Daemon submits with one [turbo_da] app_id; a second repo (530) cannot submit through it. v1 writers submit to TurboDA directly; make the daemon config per-repo later.

## Summary of Changes

sbo df21bf9: [turbo_da] gains [[turbo_da.repos]] {app_id, api_key} with env override SBO_TURBO_DA_API_KEY_<app_id>; the submit queue tags writes by DA target and the scheduler batches per target in order; /v1/submit takes ?repo=. Dokku config SBO_TURBO_DA_API_KEY_530 set on sbo-daemon 2026-09-15; mingo deploy/sbo-daemon documents it.
