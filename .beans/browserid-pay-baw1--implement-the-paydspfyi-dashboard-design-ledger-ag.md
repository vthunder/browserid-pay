---
# browserid-pay-baw1
title: Implement the pay.dsp.fyi dashboard design (ledger / agents & budgets / agreements + timeline & scorecard popups)
status: completed
type: feature
priority: normal
created_at: 2026-09-16T14:31:01Z
updated_at: 2026-09-16T15:34:58Z
parent: browserid-pay-h1f5
---

Claude Design handoff docs/design/handoff-2026-09-16/. Build option 3a: header, balance, Needs you, Ledger with running balance, Agents & budgets with rating chips, Agreements with timeline popup, reputation scorecard popup. Single static page, inline CSS/JS, no assets. Keep the browserid login plumbing.

- [x] read spec + design reference (3a, 1c, 1d, 1e)
- [x] rewrite crates/custodian/static/index.html
- [x] {{INDEX}}/{{JUDGE}} placeholders in main.rs
- [x] harness + Chrome screenshot comparison (desktop + 390px)
- [x] dark mode derived from the palette
- [x] cargo + demo-local green
- [x] deploy and verify prod

## Summary of Changes

Rebuilt crates/custodian/static/index.html as option 3a: header + balance, Needs you strip, Ledger with running balance, Agents & budgets with rating chips and grouped per-agreement caps, Agreements (active first, settled collapsed) with the four card states, plus the timeline popup (chain events, money-flow bar, ruling block) and the reputation scorecard popup. {{INDEX}}/{{JUDGE}} added to the page template in main.rs. Dark mode derived from one token set (all text ≥5.5:1). Verified with Playwright against a fixture harness: computed styles match the design tokens, console clean, phone layout at 390px, popups open/close, inner links preserved.

## Review round (Dan, 2026-09-16)

Six changes: sign out moved into a header identity dropdown (footer line deleted); the timeline keeps the reader's scroll across polls and only follows the newest event on open or while tailing (≤40px from the bottom); timeline events lead with the actor on both sides ("bob confirmed receipt") — a deliberate deviation from the artboard, which trails the name on the right column; Agents & budgets is built from the union of warrant rows and the agents seen in our agreements, so an agent whose grants predate budget tracking still shows with its rating and the agreements it acted in; an Add funds dialog modelled as funding SOURCES (deposit vs charge modes) backed by a new session-authenticated `POST /api/deposit` gated by `PAY_TEST_CREDIT` with per-request, per-account-ceiling and cooldown limits; and the ruling reasoning is clamped to four lines with a more/less toggle on both the card and the timeline.
