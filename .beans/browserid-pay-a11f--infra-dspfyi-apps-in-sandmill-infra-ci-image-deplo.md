---
# browserid-pay-a11f
title: 'Infra: dsp.fyi apps in sandmill-infra, CI image deploys, secrets'
status: completed
type: task
priority: normal
created_at: 2026-09-15T14:32:06Z
updated_at: 2026-09-16T16:26:18Z
parent: browserid-pay-h1f5
---

Hobby host (dsp.fyi is not browserid.me). Apps: pay (custodian), judge (adjudicator), index (indexer). Declare in sandmill-infra apps/hobby, secrets/*.env.age (age recipient in ~/backups/sandmill), deploy workflows by GHCR image + git:from-image, per-repo dokku deploy key. Dan: A records pay/judge/index.dsp.fyi → hobby host IP; _sbo.dsp.fyi TXT once genesis lands.

- [x] apps/hobby/pay.conf, judge.conf, index.conf
- [x] secrets
- [x] deploy workflows in browserid-pay (build images; release by hand until the deploy key is authorized)
- [x] dokku deploy key + repo secret (verified 2026-09-16; CI releases on every push to main)
- [x] DNS (Dan, wildcard)
- [x] released pay/judge/index 2026-09-15, letsencrypt on; host logged into GHCR with a read:packages token (packages are private)

## Summary of Changes

The last item — the dokku deploy key and repo secret — is done, so releases are no longer by hand.

Verified 2026-09-16:

- Repo secret `DOKKU_SSH_KEY` (set 2026-09-15) and repo variable `DOKKU_HOST = sandmill.org`.
- Four workflows: `deploy-pay`, `deploy-judge`, `deploy-index`, `deploy-relay`, each building for GHCR and releasing over SSH with `dokku@$DOKKU_HOST git:from-image <app> $IMAGE:$GITHUB_SHA`, tolerating "No changes detected".
- Every push to main today released green, with the "Deploy to dokku" step actually running — most recent `deploy-pay` run 35117887462 (8m3s, success); the 13:44 push released all four apps.
- All four hosts answer 200 over TLS: pay / judge / index / relay .dsp.fyi.

This is the CI-image-deploy pattern from the global notes (build in Actions, host never compiles) — ~5-8 min per release rather than an on-host Rust build.
