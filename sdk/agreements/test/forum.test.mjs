import test from "node:test";
import assert from "node:assert/strict";
import { Agreements, MemoryChain, forum, agreementRef, endorsesRef } from "../index.mjs";
import { FakeAgent, fakeVerify } from "./fakes.mjs";
import { parseEnvelope } from "../src/envelope.mjs";

const REPO = "sbo+raw://avail:turing:506/";

function sdkFor(email, chain) {
  const agent = new FakeAgent(email);
  return new Agreements({
    agent, chain, repo: REPO, verify: fakeVerify,
    obtainWarrants: agent.obtainAll, http: fetch, index: "http://127.0.0.1:1",
  });
}

/** The wire bytes MemoryChain recorded for the Nth submission. */
function headersOf(chain, n) {
  return parseEnvelope(chain.wires[n]).headers;
}

test("a post carries its agreement reference in the signed Related header", async () => {
  const chain = new MemoryChain();
  chain.wires = [];
  const origSubmit = chain.submit.bind(chain);
  chain.submit = async (w) => { chain.wires.push(w); return origSubmit(w); };

  const f = forum(sdkFor("alice@x.test", chain), { community: "agents" });
  await f.post("recommendations", "bob summarised ten papers on time. Recommended.", {
    related: [agreementRef("agr_abc"), endorsesRef("bob@y.test")],
  });

  const h = headersOf(chain, 0);
  // The reference belongs in the envelope, not the body: generic across object
  // types, and an indexer can join posts to agreements without parsing prose
  // (browserid-pay-xnok).
  assert.equal(
    h.Related,
    '[{"rel":"agreement","ref":"/agreements/agr_abc/"},{"rel":"endorses","ref":"bob@y.test"}]',
  );
  assert.equal(h["Content-Schema"], "post.v1");
  assert.match(h.Path, /^\/communities\/agents\/spaces\/recommendations\/$/);
});

test("a post with nothing to reference emits no Related header at all", async () => {
  const chain = new MemoryChain();
  chain.wires = [];
  const origSubmit = chain.submit.bind(chain);
  chain.submit = async (w) => { chain.wires.push(w); return origSubmit(w); };

  const f = forum(sdkFor("alice@x.test", chain), { community: "agents" });
  await f.post("help", "How do you handle a handoff that never arrives?");
  assert.equal(headersOf(chain, 0).Related, undefined, "an empty array must not be emitted");
});

test("joining self-issues a membership the community policy will accept", async () => {
  const chain = new MemoryChain();
  const f = forum(sdkFor("alice@x.test", chain), { community: "agents" });
  await f.join();

  const att = await chain.get("/u/alice@x.test/attestations/alice@x.test/", "membership-agents");
  assert.ok(att, "membership lands in the agent's OWN namespace, which is what it can write to");
  // The community's role is `{attested: {type: "membership:agents"}}` with no
  // `by` — any issuer counts, which is what makes the community open. Subject
  // and issuer are both the agent: it admits itself.
  assert.equal(att.payload.type, "membership:agents");
  assert.equal(att.payload.subject, "alice@x.test");
  assert.equal(att.payload.issuer, "alice@x.test");
});

test("reading a space threads replies under their post, newest post first", async () => {
  const chain = new MemoryChain();
  const alice = forum(sdkFor("alice@x.test", chain), { community: "agents" });
  const bob = forum(sdkFor("bob@y.test", chain), { community: "agents" });

  const older = await alice.post("needs", "Need ten papers summarised by Friday.");
  const newer = await alice.post("needs", "Need a logo.");
  await bob.reply("needs", older.id, "I can do the papers.");

  const feed = await alice.read("needs");
  assert.deepEqual(feed.map((p) => p.body), ["Need a logo.", "Need ten papers summarised by Friday."]);
  assert.equal(feed[0].replies.length, 0);
  assert.equal(feed[1].replies.length, 1);
  assert.equal(feed[1].replies[0].body, "I can do the papers.");
  assert.equal(feed[1].replies[0].by, "bob@y.test");
  assert.equal(newer.id.startsWith("p_"), true);
});

test("a space's _config is not a post", async () => {
  const chain = new MemoryChain();
  chain.put("/communities/agents/spaces/needs/", "_config", { durability: "batched" }, "sys");
  const f = forum(sdkFor("alice@x.test", chain), { community: "agents" });
  await f.post("needs", "a real post");
  const feed = await f.read("needs");
  assert.equal(feed.length, 1, "the collection config must not show up as a post");
  assert.equal(feed[0].body, "a real post");
});

test("an empty post is refused rather than written", async () => {
  const f = forum(sdkFor("alice@x.test", new MemoryChain()), { community: "agents" });
  await assert.rejects(() => f.post("needs", "   "), /needs a body/);
  await assert.rejects(() => f.reply("needs", "p_1", ""), /needs a body/);
});
