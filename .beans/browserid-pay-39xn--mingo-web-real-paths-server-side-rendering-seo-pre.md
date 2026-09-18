---
# browserid-pay-39xn
title: 'mingo-web: real paths + server-side rendering (SEO precondition)'
status: draft
type: task
priority: normal
created_at: 2026-09-17T23:51:30Z
updated_at: 2026-09-18T00:19:10Z
parent: browserid-pay-fors
---

Precondition for the SEO argument in the parent epic — indexed threads surfacing in ordinary topic searches, the way Reddit does.

**Two blockers, not one** (verified 2026-09-18):

1. `mingo-web` is a pure client-side SPA — `app.js` (~125KB) reads confirmed state from the daemon `/v1` API and renders in-browser. No HTML for a crawler.
2. It uses **hash routing** (`index.html`: `data-route="#/"`). Fragments are not distinct URLs, so today every post in mingo shares a single indexable URL. SSR alone would not fix this.

So: real paths first, then server-side rendering. Mechanical gruntwork rather than design work, but the URL scheme should be chosen before anything starts linking to posts.

Not urgent — the SEO benefit cannot kick in until there is content worth indexing — but it gates that benefit entirely.

- [ ] URL scheme for communities, spaces, threads and posts
- [ ] SSR approach: does it keep app.js for interaction and hydrate, or split read/write paths
- [ ] writes stay client-side signed against the daemon — SSR is read-path only
- [ ] sitemap / crawlability, and what a bot sees for a pending-vs-confirmed object
- [ ] do it together with the SBO URI resolver+renderer rather than after

## Decision (2026-09-18): a separate read-only render path, not a rebuild

The question was whether to rebuild the frontend in a framework with isomorphic rendering rather than retrofit crawlability. **Neither** — add a small server-side read path and leave `app.js` alone apart from routing.

**What `app.js` actually is** (inspected 2026-09-18): 2454 lines of **vanilla JS, no framework** — the "React" grep hit is `payloadReaction`. 107 functions, of which only about ten are views: `viewHub`, `viewCommunity`, `viewThread`, `viewModerate`, `viewPassport`, `viewSettings`, plus `renderChrome` / `feedRow` / `renderReceipt`.

The other ~95 are write path and session machinery: `ensureSigner`, `signEnvelope`, `sbo()` wasm, `ensureDnssecFresh`, `certIssuer`, the delegated-poster flow (`enablePoster`, `pollPoster`, `submitViaPoster`), moderation, drafts, live polling, receipts, passport. **None of it can be server-rendered** — writes are signed in-browser — and it is the hard-won part.

So a rebuild ports ~95 functions of signing and session logic that gain nothing from SEO, to fix three views. Wrong trade.

**What crawlers need is three read views** — hub, community, thread — rendered server-side from the daemon's `/v1` API. That is a small read-only service, not a port of `app.js`.

**Two reinforcing points:**

- The SBO URI resolver (`browserid-pay-xn6z`) must run server-side too, or embedded agreements and agent cards are invisible to crawlers. The SSR service is where it lives — the concrete reason these two beans are one project.
- The only change `app.js` genuinely needs is **routing**: hash → History API real paths, touching `route` and the `data-route` call sites. Architecture untouched.

**The cost, accepted:** two renderers for the same content, and the drift that invites. Mitigated by making the server view deliberately simpler — read-only, no vote buttons, no compose — so it is a crawler and first-paint surface rather than a second implementation of the app. `app.js` takes over on load.

**When a rebuild would be right:** if the duplication turns out to be the real pain, pick something isomorphic and run one view layer on both sides. That is a "we dislike the drift" reason, not an SEO one, and it costs the signing code a rewrite. Not now.

- [ ] URL scheme for communities, spaces, threads and posts — choose before anything links to posts
- [ ] `app.js`: hash routing → History API real paths
- [ ] read-only SSR service over the daemon `/v1` API: hub, community, thread
- [ ] server-side SBO URI resolution so embeds are crawlable (`browserid-pay-xn6z`)
- [ ] what a crawler sees for a pending-vs-confirmed object
- [ ] sitemap / crawlability
- [ ] keep the server view read-only; writes stay client-side signed
