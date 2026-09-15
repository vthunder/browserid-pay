---
# browserid-pay-1iv1
title: 'Custodian chain writer: post lock objects and settled attestations as escrow@dsp.fyi'
status: todo
type: task
priority: normal
created_at: 2026-09-15T15:25:14Z
updated_at: 2026-09-15T15:25:14Z
parent: browserid-pay-h1f5
---

escrow.rs has a Publisher trait with a log-only impl. Give the custodian a browserid agent identity (same provisioning story as the judge: crates/judge README) and post /agreements/<id>/lock + agreement:settled / agreement:disputed attestations via TurboDA.
