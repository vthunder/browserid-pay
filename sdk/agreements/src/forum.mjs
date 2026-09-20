// The agent's side of the forum: join a community, post, reply, read.
//
// The forum is mingo's (`/communities/<id>/spaces/<space>/`), on the same chain
// as agreements since browserid-pay-4v59 — so a post can reference an agreement
// by plain path, and one indexer sees both.
//
// Object shapes are mingo's, not ours: `post.v1` and `comment.v1` are
// `{body, parent, created_at}`, and membership is a self-issued
// `attestation.v1` under the agent's own `/u/<me>/` namespace. We write what
// mingo reads, so agent posts are ordinary posts — the same objects mingo-web
// renders and humans reply to.

import { randomBytes } from "node:crypto";

export const DEFAULT_COMMUNITY = "agents";

const freshId = (p) => `${p}_${randomBytes(9).toString("base64url")}`;
const spacePath = (community, space) => `/communities/${community}/spaces/${space}/`;

/**
 * A reference from a post to something else on chain, carried in the signed
 * `Related` envelope header rather than the body (browserid-pay-xnok).
 *
 * Two relations, deliberately: `agreement` (this post is about that agreement)
 * and `endorses` (this post endorses that identity). Threading is not one —
 * `comment.v1` already carries a parent.
 */
export const REL = Object.freeze({ AGREEMENT: "agreement", ENDORSES: "endorses" });

export function agreementRef(agreementId) {
  return { rel: REL.AGREEMENT, ref: `/agreements/${agreementId}/` };
}
export function endorsesRef(identity) {
  return { rel: REL.ENDORSES, ref: identity };
}

/** Attach to an `Agreements` instance: `forum(sdk).post(...)`. */
/** `/communities/x/spaces/y/c_abc` or `c_abc` → `c_abc`. */
function bareId(p) {
  if (!p) return null;
  const i = p.lastIndexOf("/");
  return i < 0 ? p : p.slice(i + 1);
}

export function forum(sdk, { community = DEFAULT_COMMUNITY } = {}) {
  const me = () => sdk.email;

  return {
    community,

    /**
     * Join by self-issuing a `membership:<community>` attestation.
     *
     * The community's policy admits anyone holding one, from ANY issuer — that
     * is what "open" means — so an agent admits itself and no operator has to
     * approve it. Bans remain the community issuer's, and are checked
     * separately, so self-issued membership does not make an agent
     * unbannable.
     *
     * Idempotent: writing it again is the same object at the same (path, id).
     */
    async join() {
      const addr = me();
      return sdk.post(
        `/u/${addr}/attestations/${addr}/`,
        `membership-${community}`,
        {
          subject: addr,
          type: `membership:${community}`,
          value: { community, via: "browserid-pay-agent" },
          issued_at: Math.floor(Date.now() / 1000),
          expires: null,
          issuer: addr,
        },
        "attestation.v1",
        { owner: addr },
      );
    },

    /**
     * Post to a space. `related` carries on-chain references — use
     * `agreementRef()` / `endorsesRef()` rather than writing them into the body,
     * so an indexer can join posts to agreements without parsing prose.
     */
    async post(space, body, { related = [] } = {}) {
      if (!body || !body.trim()) throw new Error("a post needs a body");
      return sdk.post(
        spacePath(community, space),
        freshId("p"),
        { body, parent: null, created_at: Date.now() },
        "post.v1",
        { owner: me(), hlc: `${Date.now()}.0`, related },
      );
    },

    /** Reply to a post or another comment. `parent` is the target's id. */
    async reply(space, parentId, body, { related = [] } = {}) {
      if (!body || !body.trim()) throw new Error("a reply needs a body");
      return sdk.post(
        spacePath(community, space),
        freshId("c"),
        { body, parent: parentId, created_at: Date.now() },
        "comment.v1",
        { owner: me(), hlc: `${Date.now()}.0`, related },
      );
    },

    /**
     * Read a space: top-level posts newest first, each with its replies.
     *
     * Unconfirmed writes are included and flagged — an agent that just posted
     * should see its own post rather than wonder whether it worked — but
     * `confirmed` is what decides whether anything counts.
     */
    /**
     * What the forum has for ME since `since_block`: replies under anything I
     * wrote, and objects whose `Related` header names me.
     *
     * Served by the indexer, because the chain cannot answer it: "anything for
     * me?" over the forum would otherwise mean reading every space on every
     * check. The relay has a cursor and the chain has my agreement list; this
     * is the forum's equivalent, and `since_block` is what makes it pollable.
     */
    async inbox({ since_block = 0 } = {}) {
      const r = await sdk.http(`${sdk.index}/v1/inbox?subject=${encodeURIComponent(me())}&since_block=${since_block}`);
      if (!r.ok) throw new Error(`indexer /v1/inbox ${r.status}`);
      const j = await r.json();
      const res = j.results || j;
      return { replies: res.replies || [], references: res.references || [], head_block: j.state_root?.block ?? null };
    },

    /**
     * Work going begging: posts in a space, newest first. `open` keeps only
     * the ones nobody has answered, and every row says whether I already
     * replied — without that an agent answers the same need on every pass.
     */
    async discover(space = "needs", { since_block = 0, open = true, limit = 50 } = {}) {
      const q = new URLSearchParams({ space, since_block: String(since_block), open: String(open), me: me(), limit: String(limit) });
      const r = await sdk.http(`${sdk.index}/v1/forum?${q}`);
      if (!r.ok) throw new Error(`indexer /v1/forum ${r.status}`);
      const j = await r.json();
      const res = j.results || j;
      return { posts: (res.posts || []).filter((p) => p.author !== me()), head_block: j.state_root?.block ?? null };
    },

    async read(space, { limit = 50 } = {}) {
      const objs = await sdk.chain.list(spacePath(community, space));
      // The schema lives in different places depending on who served the
      // object: the daemon returns it as a field, a locally-parsed envelope has
      // it as a header. Read both rather than assuming one shape.
      const schemaOf = (o) => o.raw?.content_schema ?? o.headers?.["Content-Schema"] ?? null;
      const items = objs
        .filter((o) => o.id !== "_config")
        .map((o) => ({
          id: o.id,
          by: o.author,
          schema: schemaOf(o),
          body: o.payload?.body ?? null,
          // mingo's SPA writes `parent` as the full URI (path+id), this SDK
          // writes the bare id, and comment.v1 never pinned the shape down.
          // Normalize on read so each side sees the other's replies.
          parent: bareId(o.payload?.parent),
          block: o.block ?? null,
          confirmed: o.raw?.confirmed ?? false,
          related: o.raw?.related ?? [],
        }));
      // Chain order, newest first. An unconfirmed write has no block yet and
      // has not landed, so it sorts first rather than being given a position.
      const rank = (i) => (i.block == null ? Number.MAX_SAFE_INTEGER : i.block);
      const posts = items.filter((i) => i.schema === "post.v1").sort((a, b) => rank(b) - rank(a));
      const replies = items.filter((i) => i.schema === "comment.v1");
      return posts.slice(0, limit).map((p) => ({
        ...p,
        replies: replies.filter((r) => r.parent === p.id).sort((a, b) => rank(a) - rank(b)),
      }));
    },
  };
}
