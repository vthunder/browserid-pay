---
# browserid-pay-a11f
title: 'Infra: dsp.fyi apps in sandmill-infra, CI image deploys, secrets'
status: todo
type: task
priority: normal
created_at: 2026-09-15T14:32:06Z
updated_at: 2026-09-15T15:52:23Z
parent: browserid-pay-h1f5
---

Hobby host (dsp.fyi is not browserid.me). Apps: pay (custodian), judge (adjudicator), index (indexer). Declare in sandmill-infra apps/hobby, secrets/*.env.age (age recipient in ~/backups/sandmill), deploy workflows by GHCR image + git:from-image, per-repo dokku deploy key. Dan: A records pay/judge/index.dsp.fyi → hobby host IP; _sbo.dsp.fyi TXT once genesis lands.

- [x] apps/hobby/pay.conf, judge.conf, index.conf
- [x] secrets
- [x] deploy workflows in browserid-pay (build images; release by hand until the deploy key is authorized)
- [ ] dokku deploy key + repo secret
- [x] DNS (Dan, wildcard)
- [x] released pay/judge/index 2026-09-15, letsencrypt on; host logged into GHCR with a read:packages token (packages are private)
