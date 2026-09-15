---
# browserid-pay-w5fp
title: Writers submit through the daemon (/v1/submit?repo=) instead of TurboDA directly
status: todo
type: task
priority: normal
created_at: 2026-09-15T16:45:25Z
updated_at: 2026-09-15T16:45:25Z
parent: browserid-pay-h1f5
---

sbo df21bf9 (deployed) gives /v1/submit per-repo credentials and write-time DNSSEC evidence refresh. The judge (crates/judge), the agreements SDK (sdk/agreements/src/chain.mjs), the custodian's future chain writer (1iv1) and dsp-genesis dnssec-refresh/submit should call https://da.sandmill.org/v1/submit?repo=sbo%2Braw%3A%2F%2Favail%3Aturing%3A530%2F and drop their own TurboDA keys and freshness logic. Genesis itself may keep direct submission (chicken-and-egg before the daemon follows the repo).
