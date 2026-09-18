---
# browserid-pay-fors
title: 'SEO: make forum threads indexable (GTM / distribution)'
status: draft
type: epic
created_at: 2026-09-18T00:19:10Z
updated_at: 2026-09-18T00:19:10Z
---

Split out of `browserid-pay-qq8y` (2026-09-18). **Not a requirement for the forum to work** — purely a go-to-market / distribution play, and it cannot pay off until there is content worth indexing. Sequence it after the forum is actually used.

**The bet:** indexed threads surface in ordinary web searches on the topic, the way Reddit results do. Discovery that needs nobody to adopt a directory format or a client, and that compounds. It is slow and worth nothing at first.

**Two blockers, not one** (verified 2026-09-18):

1. `mingo-web` is a pure client-side SPA — `app.js` (~125KB, vanilla JS) reads confirmed state from the daemon `/v1` API and renders in-browser. No HTML for a crawler.
2. It uses **hash routing** (`index.html`: `data-route="#/"`). Fragments are not distinct URLs, so today every post shares a single indexable URL. SSR alone would not fix this — real paths have to come first.

**Decided: a read-only server render path, not a frontend rebuild.** `app.js` is 107 functions of which only ~10 are views; the other ~95 are in-browser signing and session machinery that cannot be server-rendered and gain nothing from SEO. See `browserid-pay-39xn` for the full reasoning.

**Dependency on epic 1:** the SBO URI resolver (`browserid-pay-xn6z`) is built there for the human-facing UI. This epic needs it to run **server-side** too, or embedded agreements and agent cards are invisible to crawlers. That is a reuse requirement, not a rebuild — hence the note in `xn6z` to keep it standalone and framework-free.

See `docs/plans/2026-09-18-agent-forum-design.md` §5a.

## Children

- [ ] mingo-web: real paths + server-side rendering (`browserid-pay-39xn`)
- [ ] run the URI resolver server-side so embeds are crawlable
- [ ] sitemap, crawlability, and what a bot sees for a pending-vs-confirmed object
