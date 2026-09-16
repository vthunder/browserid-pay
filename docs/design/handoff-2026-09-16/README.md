# Handoff: pay.dsp.fyi account page + demo view

## Overview
Redesign of the custodian account page at pay.dsp.fyi (browserid-pay, `crates/custodian/static/`), per `docs/design/2026-09-16-pay-ui-brief.md`. It replaces the current Budgets/Escrows/Receipts page with: a real ledger with running balance, an Agents & budgets section showing both grant kinds, an Agreements section with per-agreement timeline popups, a "Needs you" strip, and reputation scorecards per `docs/plans/2026-09-16-reputation-design-draft.md`. The page doubles as the demo stage: the timeline popup tells the two-agent story (alice | chain | bob).

## About the design files
`Pay UI Explorations.dc.html` is a **design reference created in HTML** — a prototype showing intended look and behavior, not production code to copy directly. The target is the custodian's existing environment: **a single static HTML page with inline CSS/JS, no build step, no framework, no external assets** (see `docs/design/current-pay-index.html` in the repo for the current page and its login plumbing, which must be kept). Recreate the design in that environment.

The design file contains three exploration rounds. **Build the newest: option `3a`** (section id `t3`) — it is the complete dashboard including the timeline popup and reputation. Options `1e`/`2b` show the same timeline as a standalone demo view with a `demoStep` slider; `1c` shows the phone layout; `1d` the four agreement-card states. Earlier rounds are context, not spec.

## Fidelity
**High-fidelity** for layout, spacing, typography, copy, and colors (light mode). Recreate pixel-perfectly. Dark mode was NOT designed — derive it the way the current page does (`prefers-color-scheme` variable swap), keeping ≥4.5:1 text contrast.

## Screens / Views

### 1. Account page (option 3a; phone layout per 1c)
Single column, max-width 680px, background `#fffdf9`, padding 36px 40px (phone: 390px wide, 20px 16px, background `#fafafa`).

Top to bottom:
- **Header row** (flex, space-between, baseline): "pay.dsp.fyi" 600 14px system-ui; right: signed-in identity in 12px ui-monospace — local part `#1a1a1a` bold, `@domain` `#8a8578`. Never truncate identities; always bold local part / quiet domain.
- **Balance**: `$113.00` in 300 58px/1.05 Georgia (fall back `ui-serif, serif`), tabular-nums. Under it, 12.5px `#8a8578`: "USD test credit · what your agents may spend, and what they did" (say "test credit" once, quietly).
- **Needs you** (only when non-empty): left border 2px `#9a3412`, padding-left 12px, 13.5px text. Bold "Needs you" in `#9a3412`, then the item in plain words ("bob must respond to alice's dispute by 10:42") + link. Items: dispute to answer, budget nearly spent, approval pending at browserid.me.
- **Section headers** (Ledger / Agents & budgets / Agreements): italic 600 12px Georgia `#8a8578` + hairline `1px solid #e7e2d6` filling the rest of the row (flex, baseline, gap 12px). 30px top margin.
- **Ledger**: one row per balance change, newest first. Grid `104px minmax(0,1fr) 72px 72px`, gap 12px, padding 9px 0, bottom border `#f0ebe0`, 13.5px. Cells: when (11px ui-monospace `#a39d8d`, e.g. "Sep 16, 8:30 PM"); what in plain words + short agreement id (11-char prefix + `…`, 10.5px mono `#a39d8d`, links to the agreement); signed amount right-aligned tabular (− `#9a3412`, + `#1a7f37`, use U+2212 minus); running `balance_after` right-aligned `#a39d8d`. Kind → words: `credit`→"Test credit", `escrow_lock`→"Locked for agreement with ‹counterparty local›", `escrow_release`→"Released — ‹memo in words›" (e.g. "bob acknowledged" / "ruled split, payee share"), `escrow_refund`→"Adjudicator fee returned" or "Refunded — ruled split, payer share". No protocol jargon.
- **Agents & budgets**: 2-col grid `repeat(auto-fit,minmax(270px,1fr))`, gap 10px — cards sit side by side on desktop, stack under ~570px. Card: white, border `#e7e2d6`, radius 10px, padding 14px 18px. Header row: identity (13px mono, bold local) + **rating chip**: "rated 25 · index.dsp.fyi", 600 11px, pill (radius 12px, padding 2px 9px, bg `#f5f1e8`, border `#e7e2d6`, text `#6b6659`), cursor pointer, hover border `#c9c2b0`; opens the scorecard popup. Inside the card: standing budgets ("may spend up to $20.00 every 30 days"; right: bold spent · remaining left; 6px progress bar, track `#f0ebe0`, fill `#1a7f37`; meta 11.5px `#a39d8d`: "resets ‹date› · expires ‹date›"), then per-agreement caps separated by top border `#f5f1e8` ("agreements up to $56.00 each with danmills+bob" · "2 grants · both fully spent"), including reputation-gated grants: "agreements up to $200.00 every 30 days with any agent rated 70 or better at index.dsp.fyi". Below the grid, 12px `#a39d8d`: grantor-aggregate rating sentence + "Budgets are granted and revoked at browserid.me/account." (link — revocation lives there, never here).
- **Agreements**: active first (locked, disputed — with a *what happens next* line), then settled. Card: white, border `#e7e2d6`, radius 10px, padding 14px 18px, whole card clickable (cursor pointer; hover border `#c9c2b0` + shadow `0 1px 4px rgba(0,0,0,.05)`) → opens timeline popup. Rows: (1) short-id chip (11px mono on `#f5f1e8`) + state pill (600 11px uppercase, letter-spacing .05em, padding 2px 7px, radius 4px) + right-aligned "settled ‹date›" 12px `#a39d8d`; (2) role sentence 13px — for same-account agreements: "Both sides are your account — alice paid, bob received. Net −1.00 after fees."; (3) terms 12.5px `#8a8578`: "price $50.00 · adjudicator danmills+judge · contribution $0.50 to fund@dsp.fyi"; (4) money-moved grid (`auto-fit minmax(150px,1fr)`, 12px, label `#8a8578` / value tabular, top border `#f5f1e8`): locked −56.00, released to bob +30.00, refunded to alice +20.00, adjudicator fee returned +5.00, custodian fee 0.50, contribution 0.50; (5) ruling quote when ruled: italic 12.5px Georgia `#444` on `#fcfaf5`, left border 2px `#d8d2c2`, radius 0 6px 6px 0, attributed "— danmills+judge"; (6) footer 12px: "public record ↗" (index link) · "dispute case ↗" (judge link, only when ruled) · right-floated "timeline →" hint `#a39d8d`.

State pills: Locked = bg `#e8eef9` text `#2f5fa8`; Disputed = bg `#fdf3e7` text `#9a3412` (card border `#ecd9bc`); Settled (both outcomes) = bg `#e7f2ea` text `#1a7f37`. Card states detail (headline line, evidence chips on disputed, respond-by deadline) per option `1d`.

### 2. Timeline popup (inside 3a; standalone demo variant in 1e/2b)
Overlay `rgba(30,28,22,.4)` over the page, closes on backdrop click or ×. Panel: `#fffdf9`, radius 12px, shadow `0 12px 40px rgba(0,0,0,.25)`, max-width 600px, padding 18px 22px.
- Header: short-id chip + outcome pill + × (16px `#a39d8d`).
- Parties row: `danmills+alice` bold `#4348a8` (left) — "both agents are your account" center 10.5px `#a39d8d` — `danmills+bob` bold `#0e766e` (right).
- **Timeline pane**: `max-height 340px; overflow-y auto` (steps can be arbitrarily many), bordered `#f0ebe0`, radius 8px, bg `#fcfbf7`. Center spine 1px `#ddd7c9` at left:50%, "TIME ↓" label (600 9.5px uppercase, `#b0aca2`, bg-colored padding to break the spine) at top, "▼" at bottom. Events from the public index entry, in chain order. alice events: card left column, left border 2px `#4348a8`, "alice" bold colored + words + 10px mono meta "block N · sha256:… sealed". bob events mirrored right (right border `#0e766e`, right-aligned). System events (escrow lock, ruling): centered card bg `#f5f2ea` border `#e5dfd0`. Spine dot per event (8px circle, actor color, box-shadow 3px in pane bg). Event words in plain language: "proposed the agreement — 10 summaries for $50.00", "accepted", "locked $56.00 — price + fees + contribution", "handed off the inputs — 10 items", "confirmed receipt", "committed the delivery", "disputed — 'only five of ten'", "responded — opened his delivery commitment, matched ✓", "ruled: split — $30.00 to bob, $20.00 back to alice". A revealed commitment flips "sealed" → "opened ✓".
- **"Where the $56.00 went"** bar: 18px tall, radius 5px, proportional segments with inset labels (600 9.5px white): to-payee `#0e766e`, back-to-payer `#4348a8`, returned fee `#8b9469`, fees/contribution `#b0aca2`. Caption 10.5px `#a39d8d`: "5.00 unused adjudicator fee returned · 0.50 custodian fee · 0.50 contribution to fund@dsp.fyi".
- **Ruling block** (ruled agreements only): "Ruling — split" 600 12.5px + "danmills+judge · case ↗"; the judge's reasoning verbatim as an italic Georgia quote; split figures "$30.00 to bob / $20.00 back to alice"; "what bob proved: delivery commitment opened, matched ✓".
- Demo drive: the standalone variant (1e) has a `demoStep` control (0–9) dimming not-yet-happened events to opacity .25 — in production, poll `/api/me` + the index every few seconds and reveal events as they land; auto-scroll the pane to the newest event.

### 3. Reputation scorecard popup (3a)
Same overlay pattern, centered, max-width 520px. Header: agent identity + ×. Headline: score in 300 44px Georgia + "of 100 · one indexer's view, yours to trust or not" 12px `#8a8578`; sub-line 11.5px `#a39d8d`: "signed by index.dsp.fyi · formula v1 · every number carries the count behind it". Component rows (grid `150px 44px 1fr`, 12.5px, top border `#f0ebe0`): completion 0.50 "0 counted agreements — both demo runs are same-account on both sides, so they score zero"; dispute record 1.00 "no rulings against"; responsiveness 1.00 "no disputes to answer yet"; promptness 1.00 "no late or partial findings"; distinct counterparties 0 "controlling accounts, not people". Footer caveat: "Counts controlling accounts, not people — a fresh identity starts at 25, not 0 or 100." + "full card at index.dsp.fyi ↗". Data from `GET https://index.dsp.fyi/v1/reputation?subject=‹identity›`. Wording rule: say "controlling account"/"grantor", never "human".

## Interactions & behavior
- Agreement card click → timeline popup (row-level; keep inner links working via stopPropagation). Rating chip click → scorecard popup. Backdrop or × closes. One popup at a time.
- Hovers as specified (border `#c9c2b0`, subtle shadow). No animations required; a 150ms ease on popup opacity is fine.
- Keep the existing browserid login flow (`navigator.id.watch`, `/api/login`, `/api/logout`) and the signed-out state from `current-pay-index.html`.
- Responsive: single column; agents grid collapses; ledger becomes stacked two-line rows at phone width (per option 1c: what + when·id left, amount + balance right). Must work at 390px.
- Empty states in plain words ("No agent has spent from this account yet…"), matching current page tone.

## State management
Vanilla JS: `me` (from `GET /api/me`), `openAgreementId` (timeline popup), `repIdentity` (scorecard popup), fetched index entry + judge case cached per agreement, fetched reputation card per identity. Poll `/api/me` (and open agreement's index entry) every few seconds when the page is visible.

## Data
`GET /api/me` per the brief's contract — `docs/design/sample-me.json` is the realistic fixture. On-chain timeline from `GET https://index.dsp.fyi/v1/agreements/‹id›`; dispute detail (`respond_by`, reveals with `matched`) from `GET https://judge.dsp.fyi/api/cases/‹id›`; reputation from the indexer. Amounts are decimal strings with currency. **Note:** the fixture's `agreement.money.fees_paid` disagrees with its ledger on the ruled agreement's adjudicator fee; the design follows the ledger (fee returned).

## Design tokens (light mode)
Colors: page bg `#fffdf9`; card bg `#fff`; ink `#1a1a1a`; secondary `#8a8578`; faint `#a39d8d`; hairline `#e7e2d6`; row hairline `#f0ebe0` / `#f5f1e8`; credit/ok `#1a7f37`; debit/warn `#9a3412`; link `#2f5fa8` (hover `#1d4485`); alice accent `#4348a8`; bob accent `#0e766e`; system/judge neutral `#6b6659` on `#f5f2ea`; bar extras `#8b9469`, `#b0aca2`; disputed tint `#fdf3e7`/`#ecd9bc`; locked tint `#e8eef9`.
Type: system-ui stack for UI; Georgia/ui-serif for the balance figure, section labels (italic), and ruling quotes; ui-monospace/Menlo for identities, ids, hashes, timestamps. Sizes used: 58/44 (display), 14–15 (headers), 13–13.5 (body), 12–12.5 (secondary), 11–11.5 (meta), 9.5–10.5 (micro labels). `font-variant-numeric: tabular-nums` on all money columns.
Spacing: card padding 14–18px; radius 10px cards / 12px popups / 4px chips / 12px rating pill; section gap 30px; grid gaps 10–12px.

## Assets
None. System fonts only, no images, no icon fonts — glyphs are text (↗ → ▸ ▾ × ▼ ✓ − ·). This is a hard constraint of the target environment.

## Files
- `Pay UI Explorations.dc.html` — the design reference (all three rounds; `t3`/`3a` is the spec, `1c` phone, `1d` card states, `1e` standalone demo view).
- In the repo: `docs/design/2026-09-16-pay-ui-brief.md` (brief + copy rules), `docs/design/sample-me.json` (fixture), `docs/design/current-pay-index.html` (current page: login plumbing to keep, dark-mode pattern), `docs/plans/2026-09-16-reputation-design-draft.md` (scorecard semantics).
