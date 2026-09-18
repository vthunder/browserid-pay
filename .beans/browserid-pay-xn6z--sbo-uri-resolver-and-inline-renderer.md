---
# browserid-pay-xn6z
title: SBO URI resolver and inline renderer
status: draft
type: feature
priority: normal
created_at: 2026-09-17T23:51:30Z
updated_at: 2026-09-18T00:19:34Z
parent: browserid-pay-qq8y
blocked_by:
    - browserid-pay-xnok
---

The core build item, and the thing that makes the forum feel like one product rather than three.

Everything discussed wants the same mechanism: a reference to an on-chain object, anywhere in a post, resolved and rendered by type.

| referenced | renders as |
|---|---|
| agreement | status card — terms, parties, outcome |
| agent card / listing | profile chip |
| attestation (endorsement, scorecard) | badge |
| thread / post | preview |

That is one feature, not four. It is also what lets an agreement appear as an item in a thread, or as a thread of its own, and what makes a "who are your favorite agents for X" thread render as something worth reading — replies link to cards, cards link back to threads.

Depends on the reference format settled in the backlink convention bean.

**Do it together with the mingo-web SSR work**, not after: porting the SPA and then retrofitting embeds is the more expensive order.

- [ ] resolve an SBO URI, including cross-chain (506 ↔ 530) — both are on `da.sandmill.org`
- [ ] renderer per object type; unknown types degrade to a plain link
- [ ] where resolution happens — server-side at render, client-side, or both
- [ ] caching and failure modes: a reference to an object that is gone, or on an unreachable chain
- [ ] does an embed re-render as the referenced object changes state (an agreement settling)?

## Must run server-side too (2026-09-18)

Resolution cannot be client-only. If embeds resolve in the browser, a crawler sees a bare link where an agreement card or agent card should be — and the SEO case in `browserid-pay-qq8y` is precisely that threads become worth indexing.

So the resolver runs in the read-only SSR service from `browserid-pay-39xn`, and that is the concrete reason these two are one project rather than sequential. Client-side resolution is still wanted for live updates (an embed re-rendering as an agreement settles), so plan for both from the start rather than porting one to the other later.

Note `browserid-pay-39xn` concludes **not** to rebuild the frontend: `mingo-web` is vanilla JS whose bulk is in-browser signing that cannot be server-rendered. The resolver therefore has to work in two places by design, which is an argument for keeping it a small standalone piece with no framework dependency.

## Stays in epic 1 (2026-09-18 split)

When `browserid-pay-qq8y` split into the forum epic and the SEO epic (`browserid-pay-fors`), this bean stayed with the forum. The client-side renderer is what makes a thread about an agreement worth reading for a **human**, which is the legibility claim in the forum epic — it is not an SEO feature.

`browserid-pay-fors` needs the same resolver running **server-side**. That is reuse, not a rebuild: the render decision in `browserid-pay-39xn` is a small standalone read-only service, not a port of `app.js`, so it can simply call this. Which is the reason to keep this piece **standalone and framework-free** — it has to run in a browser and in a server process.

*(This supersedes the earlier note that the resolver and the render work must be one project. That was written when the assumed render approach was porting the SPA and retrofitting embeds into it; the read-only-service decision removes the retrofit, and with it the coupling.)*
