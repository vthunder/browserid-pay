import test from "node:test";
import assert from "node:assert/strict";
import { Agreements, MemoryChain, generateSealingKey, collectionAudience, contractAudience } from "../index.mjs";
import { FakeAgent, fakeVerify, fakeServers } from "./fakes.mjs";

const REPO = "sbo+raw://avail:turing:530/";

function world(servers) {
  const chain = new MemoryChain();
  const jk = generateSealingKey();
  chain.put("/sys/", "contribution", { recipient: "fund@dsp.fyi", suggested_percentage: 1.0 });
  chain.put("/providers/custodians/escrow@dsp.fyi/", "listing", { v: 1, identity: "escrow@dsp.fyi", origin: servers.origin, price: { amount: "0.50", currency: "USD" } });
  chain.put("/providers/adjudicators/judge@dsp.fyi/", "listing", { v: 1, identity: "judge@dsp.fyi", origin: servers.origin, price: { amount: "5.00", currency: "USD" }, sealing_key: jk.publicKey });
  const mk = (e) => { const a = new FakeAgent(e); return new Agreements({ agent: a, chain, repo: REPO, verify: fakeVerify, obtainWarrants: a.obtainAll, http: fetch, index: "http://127.0.0.1:1" }); };
  return { chain, alice: mk("alice@x.test"), bob: mk("bob@y.test") };
}

const TERMS = {
  title: "Haiku: a lighthouse in winter",
  obligations: [{ by: "bob@y.test", what: "one original haiku", kind: "deliverable", deadline: 1_900_000_000 }],
  price: { amount: "2.00", currency: "USD" },
  evidence: "a delivery commitment plus the text sent off-chain",
};

test("a proposal made under a standing grant can be accepted by the counterparty", async () => {
  const servers = await fakeServers();
  try {
    const w = world(servers);
    // Alice approves ONE blanket card, then proposes with no per-agreement card.
    await w.alice.requestStandingConsent({ cap: { amount: "50.00", currency: "USD" }, window: "P30D" });
    const p = await w.alice.propose(TERMS, { acceptor: "bob@y.test" });

    // Her bundle is minted for the COLLECTION audience, not this agreement's.
    assert.ok(w.alice.agent.warrantedAudiences().includes(collectionAudience(REPO)));
    assert.ok(!w.alice.agent.warrantedAudiences().includes(contractAudience(p.id, REPO)));

    // Bob must accept it. Before the fix this threw:
    //   bad_bundle: assertion: audience mismatch
    const a = await w.bob.accept(p.id, p.terms);
    assert.ok(a, "acceptance goes through under a standing grant");
  } finally {
    await servers.close();
  }
});

test("standing authority carries the whole lifecycle, both sides", async () => {
  const servers = await fakeServers();
  try {
    const w = world(servers);
    // BOTH parties on blanket grants, no per-agreement card anywhere.
    await w.alice.requestStandingConsent({ cap: { amount: "50.00", currency: "USD" }, window: "P30D" });
    await w.bob.requestStandingConsent({ cap: { amount: "50.00", currency: "USD" }, window: "P30D" });

    const p = await w.alice.propose(TERMS, { acceptor: "bob@y.test" });
    await w.bob.accept(p.id, p.terms);
    await w.alice.fund(p.id);           // mints for the collection audience
    await w.bob.deliver(p.id, { content: "salt light and silence" });
    const done = await w.alice.settle(p.id);  // acknowledge, same authority
    assert.ok(done, "settle goes through under a standing grant");

    // Neither side was ever asked for a per-agreement card: one card each.
    assert.equal(w.alice.agent.cards.length, 1, "alice approved exactly one card");
    assert.equal(w.bob.agent.cards.length, 1, "bob approved exactly one card");
  } finally {
    await servers.close();
  }
});

test("a one-off warrant is still preferred when the agent holds one", async () => {
  const servers = await fakeServers();
  try {
    const w = world(servers);
    await w.alice.requestStandingConsent({ cap: { amount: "50.00", currency: "USD" }, window: "P30D" });
    const p = await w.alice.propose(TERMS, { acceptor: "bob@y.test" });
    // Hand alice a narrower warrant for this agreement after the fact.
    w.alice.agent.grant(contractAudience(p.id, REPO), [{ scope: "contract:agreement", cap: { amount: "10.00", currency: "USD" } }]);
    await w.bob.accept(p.id, p.terms);
    await w.alice.fund(p.id);
    assert.ok(true, "the narrower authority is used without breaking the flow");
  } finally {
    await servers.close();
  }
});
