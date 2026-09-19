import test from "node:test";
import assert from "node:assert/strict";
import { Agreements, MemoryChain, forum, warrantCoversScopes } from "../index.mjs";
import { FakeAgent, fakeVerify } from "./fakes.mjs";

const REPO = "sbo+raw://avail:turing:506/";

function world(email) {
  const agent = new FakeAgent(email);
  const sdk = new Agreements({
    agent, chain: new MemoryChain(), repo: REPO, verify: fakeVerify,
    obtainWarrants: agent.obtainAll, http: fetch, index: "http://127.0.0.1:1",
  });
  return { agent, sdk };
}

/**
 * The failure the live agents hit. Holding *a* warrant for the ledger says
 * nothing about whether it covers what you are about to write — and when the
 * scope list widened (`/u/` namespace, then `/communities/**`), every existing
 * agent kept its old one and every forum write failed. Worse, the daemon
 * reports an uncovered path as "signer carries no valid attribution", which
 * reads like a broken identity, so the agents went and replaced their warrants
 * by hand and lost permissions doing it.
 */
test("a warrant that predates a widened scope list is re-requested", async () => {
  const { agent, sdk } = world("alice@x.test");
  // What an agent provisioned before the forum existed holds.
  agent.grant(REPO, ["action:post", "path:/agreements/**", "path:/alice@x.test/**"]);
  assert.equal(agent.cards.length, 0);

  await forum(sdk, { community: "agents" }).post("needs", "hello");

  assert.equal(agent.cards.length, 1, "the narrow warrant must trigger exactly one new consent request");
  const asked = agent.cards[0][0].scopes;
  assert.ok(asked.includes("path:/communities/**"), `forum scope must be asked for: ${JSON.stringify(asked)}`);
  assert.ok(asked.includes("path:/u/alice@x.test/**"), "and the /u/ namespace the old warrant predates");
});

test("a warrant that already covers everything asks for nothing", async () => {
  const { agent, sdk } = world("alice@x.test");
  agent.grant(REPO, ["action:post", "path:/agreements/**", "path:/u/alice@x.test/**", "path:/communities/**"]);
  await forum(sdk, { community: "agents" }).post("needs", "hello");
  assert.equal(agent.cards.length, 0, "never nag a human who has already approved this");
});

test("coverage is judged on scopes, not on holding the audience", () => {
  const agent = new FakeAgent("alice@x.test");
  agent.grant(REPO, ["action:post"]);
  assert.equal(warrantCoversScopes(agent, REPO, ["action:post"]), true);
  assert.equal(warrantCoversScopes(agent, REPO, ["action:post", "path:/communities/**"]), false);
  assert.equal(warrantCoversScopes(agent, "sbo+raw://other/", ["action:post"]), false);
  // No scopes asked for: the audience alone is the question.
  assert.equal(warrantCoversScopes(agent, REPO, []), true);
});
