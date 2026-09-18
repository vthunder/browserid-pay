---
# browserid-pay-9dhg
title: Genesis runbook gaps found by the v6 regenesis
status: todo
type: task
priority: normal
created_at: 2026-09-18T20:34:26Z
updated_at: 2026-09-18T20:34:26Z
---

Both found by actually running the v6 regenesis of the shared chain on 2026-09-18 (`browserid-pay-4v59`), which is what the notes were written for.

## 1. `mingo genesis` drops a SECRET into the repo root

It writes the checkpoint authority key to `checkpoint-key.json` (default for `--checkpoint-key-out`) in the working directory — untracked, mode 644, sitting next to the code in a **public** repo. Nothing in the runbook says to remove it, and `.gitignore` does not cover it.

It is written even when `--checkpoint-key <alias>` reuses an existing keyring key, so the file is pure risk in the common case: the secret is already in the keyring and already deployed as the daemon's `SBO_CHECKPOINT_KEY`.

- [ ] add `checkpoint-key.json` to mingo's `.gitignore` (cheapest guard)
- [ ] do not write the file at all when `--checkpoint-key` names an existing alias, or write it to a path outside the repo
- [ ] mode 600 when it is written
- [ ] the runbook should say to delete it once deployed

## 2. The runbook's submit path assumes direct TurboDA

`deploy/GENESIS.md` → "Reproduce / recover" ends at building `genesis.wire`, and the tooling's hint is `curl … <daemon>/v1/submit`. But submitting **through the daemon returns no submission id** — `{"submission_id":"","accepted":true,"pending":true,…}` — and the block number is what every following step needs (`first_block`, `head`, `expected_genesis`, the canonical identity, the DNS record).

The id only appears in the daemon's own log:
```
sbo_daemon::turbo: Submitted! submission_id: b2554fe5-…
```
so the runbook's next step is undocumented and requires shell access to the host. `dsp-genesis status <id>` then gives `block_number`.

- [ ] document one path end to end: either submit directly to TurboDA (and say where the key comes from), or submit via the daemon and say to read the id from its log
- [ ] better: have `/v1/submit` return the TurboDA `submission_id` it already has
- [ ] record that `dsp-genesis status` needs `AVAIL_TURBODA_KEY`, which lives in the daemon's dokku config

## What the notes got RIGHT, worth keeping

- reusing keyring aliases kept sys, domain and checkpointer keys identical, so admin identity and the daemon's `[checkpoint]` key survived untouched — verified against the live chain before building
- the `RESET_MARKER` bump is a clean one-shot wipe, and it retired the second database in the same pass
- `expected_genesis` verification worked: *"Genesis verified for repo f86a7b415defc6cf at block 3899192"*
