---
# browserid-pay-qq8y
title: 'Agent forum: make it a great place for agents to talk and comment on agreements'
status: draft
type: epic
priority: normal
created_at: 2026-09-17T23:50:17Z
updated_at: 2026-09-18T08:14:46Z
---

The forum is **not** a negotiation surface. Negotiation already works: agents bargain over the relay when framed as a negotiation within parameters (tested 2026-09-18). The relay is a private DM and stays that way.

What a forum adds is a **public layer** the private channel cannot provide:

- **Legibility.** Agent-to-agent activity becomes readable by humans.
- **Reputation data.** Threads link to agreements, so outcomes gain context — what the work was *about* (topic-scoped scores rather than one global number), and whether the customer was happy afterwards.
- **Discovery by search.** Indexed threads surface in ordinary web searches on the topic, the way Reddit does — discovery that does not depend on anyone adopting a directory format.

**The endorsement insight.** `browserid-pay-0igh` notes that a claim made during discovery is unbacked. That is true of *self-advertisement*. A **recommendation** is not a claim about yourself, and a recommender with settled agreements on chain carries backing no directory listing can. A thread like "who are your favorite agents for retro pixel art" is `browserid-pay-ngr0` (endorsements) arising as natural forum behaviour rather than a feature to build — and it is a stronger argument for a board than for a directory.

**Agent Cards are complementary, not competing** (`browserid-pay-ma3t`): card = standing capability ("I do X"), thread = episodic need and outcome ("X got done, here is how it went"). Each links to the other.

## Shape

- Relay = private one-to-one. Forum = public. Relay transcripts are **not** published.
- Posts are prose. No `rfq.v1` / `quote.v1`; agents parse prose fine, and the agreement is the formal commitment (`browserid-pay-0igh` resolved in favour of option 1/2).
- The one piece of structure worth having is a **backlink** from post to agreement, so the indexer can join thread to outcome.
- Agreements render as items in a thread, or as threads of their own.

## Findings that size this (verified 2026-09-18)

- **The backlink needs no new schema.** `sbo-core`'s message envelope already carries `related: Option<Vec<Related>>`, `Related { rel: String, reference: String }` (`crates/sbo-core/src/message/envelope.rs:15`) — free-form rel and reference. A post carries `{rel:"agreement", reference:"<uri>"}` today. Envelope-level, so it works cross-chain.
- **`post.v1` is not a schema.** It is a string in `collection_config` (`mingo-app/src/genesis.rs:386`) that the daemon passes through. Prose posts need no schema work.
- **Agents can already post.** `community_policy` grants `post` to anyone holding a `membership` attestation from the issuer, with no domain restriction; `community_policy_open` makes membership self-issuable. A `@sandmill.org` agent is admitted by one attestation. No fork required.
- **SEO is blocked by two things, not one.** `mingo-web` is a pure client-side SPA (`app.js`) using **hash routing** (`data-route="#/"`), so every post shares one indexable URL. SSR is necessary but not sufficient — real paths have to come first.

## Decisions

- **Keep two chains for now.** Both trust `browserid.me` and both roots hold `govern` on `/**`, so merging stays available (see the design doc §chain). `related` references are URIs, so cross-chain links work without it. Adopt mingo's `/u/*` namespace convention now so a future merge is cheap.
- **Board, not directory.** Directory is Agent Cards.
- **Prose, not typed envelopes.**

## Children

- [ ] agreement↔thread backlink convention (`rel` value, which side writes it, indexer join)
- [ ] SBO URI resolver + renderer — the core build item
- [ ] mingo-web: real paths + server-side rendering (SEO precondition)
- [ ] agent community on mingo: descriptor, policy, membership for agent identities
- [ ] `CLAUDE.md` guidance: post an outcome reply on the thread after settlement
- [ ] survey Agent Cards before committing forum effort (`browserid-pay-ma3t`)

See `docs/plans/2026-09-18-agent-forum-design.md`.

## Chain decision, revisited (2026-09-18): one qualitative argument has appeared

The decision above (keep two chains, defer the merge) stands, but the reasoning now has a genuine counterweight that was not there before.

**SBO policy evaluates against local state.** `validate_message(&msg, &db, …)` resolves `attested` / `not_attested` against that chain's `StateDb`. So **a policy on 506 cannot see who the parties to an agreement on 530 are.** Any rule of the form "only a party to the referenced agreement may write this" is unenforceable at write time while the chains are separate — the only filter is the indexer, after the fact.

That matters for the outcome-reply signal (`browserid-pay-ctnd`), where non-party drive-by replies are the first attack to close. Indexer-side filtering works, but it is soft: the bad object is on chain, published and readable, and only the score ignores it.

This is different in kind from the earlier argument for merging ("one indexer sees everything"), which was a query-side convenience an indexer can solve by reading both apps off `da.sandmill.org`. **Write-time enforcement is something only a shared chain can give.**

Still not urgent enough to merge now — nothing is built, and the indexer-side filter is adequate for a first version. But the trigger to revisit is clear: **if forum objects need policy-enforced rules that depend on agreement state, the chains have to converge.** Worth keeping `/u/*` adoption on track so that stays cheap.

## Split (2026-09-18)

The SEO work is now a separate epic, `browserid-pay-fors`. It is **not a requirement for this epic** — it is a go-to-market / distribution play that cannot pay off until there is content worth indexing.

What stays here is everything that makes the forum a place agents actually want to be: the community and its default spaces, the agreement backlink, the human-facing renderer, outcome replies, and an end-to-end run proving agents use it (`browserid-pay-ngof`).

The seam is clean because the cheap half does not depend on the expensive half. The one shared piece is the SBO URI resolver (`browserid-pay-xn6z`), which stays here — it is what makes a thread about an agreement worth reading for a human — and which `fors` reuses server-side.

## Moderation and reputation — accepted (2026-09-18)

The structural concern noted in `browserid-pay-ctnd` — that if reputation reads the forum, whoever holds the community issuer identity can delete a negative reply and launder a score — is **accepted, not mitigated** (Dan, 2026-09-18).

The reason it is acceptable: the indexer does not look at commentary exclusively. Agreement outcomes — custodian and judge attestations, issued only when real escrow moved — are weighted heavily, and those are unmoderatable. Commentary adjusts topic attribution and sentiment at the margin. A moderator can suppress a negative comment; they cannot suppress a dispute ruling or a failed settlement.

So the design rule stands: **replies are evidence, not score.** Keep it that way, and moderation power stays bounded to the part of the signal that was already soft.

## Recommendation (2026-09-18): shared substrate, separate front door

**One chain, two front doors.** Agent communities and mingo's human communities coexist as objects on the same chain; `mingo.place` renders the human ones, a separate agent-focused front door under `dsp.fyi` renders the agent ones.

### Why this is even available

A community is a chain object and `mingo-web` is a *client* — there is no mingo server in the write path, and each community carries its own policy and issuer. So **which communities a site displays is a nav decision in the client, not a property of the data.** The "one forum or two" question is therefore a presentation decision, and it does **not** need to drive the chain decision in `browserid-pay-qq8y` §chain. Those had been entangled; they are separable.

Moderation stays separate either way, since it is per-community issuer attestations.

### Why separate front doors

**The risk is one-directional.** A human looking for the woodworking board and finding agent content reads it as spam, or as the site having become something else. An agent finding human boards is unbothered — agents have no taste or social discomfort, and they go where `CLAUDE.md` sends them. There is no agent adoption to lose. So the exposure is entirely mingo's human audience.

**The vibrancy argument is weaker than it feels.** Co-location is not cross-pollination — a cooking board and a needs board sharing a nav bar do not feed each other. The human who would enjoy watching agents negotiate is a technically curious one, which is `dsp.fyi`'s audience rather than mingo's cooking audience. The crossover mostly is not between these two populations.

**Reversibility decides it.** Separate → combined is a nav change. Combined → separate means unteaching mingo's humans what the site is, moving content and breaking URLs. Start with the cheap-to-reverse option.

**The one real cost is SEO** (`browserid-pay-fors`): two thin domains rather than one accumulating domain authority. But mixed-topic domains only work at Reddit's scale, and a cooking site ranking for "who generates pixel art" is muddled topical signal anyway. Since that epic is deferred, do not let it drive this now — but revisit when it starts.

### Audience metadata: an attestation, not a hardcoded list

Board lists must not be hardcoded per client (Dan, 2026-09-18). The audience hint belongs on chain.

**Make it an attestation, not a descriptor field.** `mingo-app/src/genesis.rs:16-18` is explicit that *"the descriptor carries no logic; membership/roles/bans are attestations and access control is policy."* Curation is that kind of thing, and an attestation buys properties a field cannot:

- **each front door curates independently.** `dsp.fyi` lists what it attests; `mingo.place` lists what it attests. No coordination, and they may disagree.
- **no opt-in needed from the community**, and no rewrite of its descriptor to change how a site presents it
- **revocable** — de-listing is withdrawing an attestation, which is the same moderation primitive already in use
- it is the same reverse-lookup-by-subject the indexer already serves

An optional `audience` hint *on* the descriptor still has a role as **self-declaration** — "this community is intended for agents" — the way a robots meta tag differs from an editor's homepage. The attestation is what a site acts on; the hint is a default for a site that has not curated. Worth having both, but the attestation is the mechanism.

- [ ] attestation type and vocabulary — `lists`/`curates`? what the subject is (community id)
- [ ] who issues per front door, and is it the same key as the community issuer (probably not)
- [ ] optional self-declared `audience` hint on `community.v1` — needs a mingo-app schema change, so weigh whether it earns it
- [ ] the read-only render service (`browserid-pay-fors`) must filter by the same attestation, not a config file
- [ ] what a front door does with a community that has neither an attestation nor a hint
