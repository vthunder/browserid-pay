---
# browserid-pay-twbm
title: 'mingo: HLC times are author-asserted with a 24h backdating window'
status: todo
type: task
priority: low
created_at: 2026-09-18T19:25:26Z
updated_at: 2026-09-18T19:25:26Z
---

Tracked here, not in mingo: mingo is a **public** repo and this is private, so anything that describes what browserid-pay does stays on this side (see `deploy/GENESIS.md` → "Keep this out of the shared repo"). These are observations about mingo's code; act on them there, but do not file them there.

Noticed while auditing timing across the stack for `browserid-pay-e1ou` (where obligation deadlines turned out to derive from an unverified payload field — a genuine bug). **mingo is in much better shape**: it carries an HLC on every write, so its times are at least *bounded*. But the bound is loose, and two places do more with the HLC than display it.

**What mingo does today:**

- writes set `hlc: hlc || `${Date.now()}.0`` (`app.js:473` poster path, `:858` direct path) — the browser's clock
- display goes through `hlcMs()` (`:1134`) → `relTime` / `timeAgo`
- feed and thread ordering sorts on the HLC string (`:1467`, `:1568`, `:1710`)
- reaction state resolves last-write-wins by HLC comparison (`:169`)

Block height is never used for time, and no height→time mapping exists anywhere (the daemon has the block timestamp at sync but does not persist it — `sbo-xgyr`).

**The bound.** The daemon enforces `T_b − W ≤ physical ≤ T_b + ε`, and mingo's spaces set `max_authoring_lag_s: 86400`, so a post may claim to be **up to 24 hours old**. Forward skew `ε` is 5 minutes.

For a forum this is mostly the right trade — the wide `W` is deliberate, it is what let the seed corpus import with historical timestamps (`seed.rs`, `WIDENED_LAG_S`). Two consequences worth knowing rather than fixing today:

- **Sort gaming.** Forward-dating by up to `ε` (5 min) pins a post to the top of a newest-first sort. Harmless at human volume; a cheap spam lever if agent-written posts arrive in bulk (`browserid-pay-qq8y` assumes many agents).
- **Reaction LWW.** The same +5 min trick wins a contested reaction state at `:169`. Low stakes — it is a vote, not money.

Neither is urgent. Filing so the reasoning exists before someone copies the forum's loose `W` onto a collection where it would matter: **`W` is the slack in any deadline built on it**, which is exactly how browserid-pay got into trouble.

- [ ] decide whether feed ordering should tolerate `ε`-scale gaming, or sort by something the author does not control
- [ ] if agent volume arrives, revisit — spam economics differ from human posting
- [ ] do not narrow `W` without checking the seed path, which depends on the wide window
