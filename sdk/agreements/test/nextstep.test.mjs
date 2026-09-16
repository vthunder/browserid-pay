import test from "node:test";
import assert from "node:assert/strict";
import { Agreements, MemoryChain, generateSealingKey } from "../index.mjs";
import { FakeAgent, fakeVerify, fakeServers } from "./fakes.mjs";

const REPO = "sbo+raw://avail:turing:530/";

function world(servers) {
  const chain = new MemoryChain();
  const judgeKey = generateSealingKey();
  chain.put("/sys/", "contribution", { recipient: "fund@dsp.fyi", suggested_percentage: 1.0 });
  chain.put("/providers/custodians/escrow@dsp.fyi/", "listing", { v: 1, identity: "escrow@dsp.fyi", origin: servers.origin, price: { amount: "0.50", currency: "USD" } });
  chain.put("/providers/adjudicators/judge@dsp.fyi/", "listing", { v: 1, identity: "judge@dsp.fyi", origin: servers.origin, price: { amount: "5.00", currency: "USD" }, sealing_key: judgeKey.publicKey });
  const mk = (email) => {
    const agent = new FakeAgent(email);
    // No indexer in tests: `mine()` falls back to scanning the chain.
    return new Agreements({ agent, chain, repo: REPO, verify: fakeVerify, obtainWarrants: agent.obtainAll, http: fetch, index: "http://127.0.0.1:1" });
  };
  return { chain, alice: mk("alice@x.test"), bob: mk("bob@y.test") };
}

// An input alice must hand over, then a deliverable from bob that depends on it.
const TERMS = {
  title: "Summarize 10 papers",
  obligations: [
    { by: "alice@x.test", what: "the ten papers", kind: "input" },
    { by: "bob@y.test", what: "deliver summaries", kind: "deliverable", depends_on: 0, deadline: 1_900_000_000 },
  ],
  price: { amount: "50.00", currency: "USD" },
  evidence: "a delivery commitment plus the summaries sent off-chain",
};

test("next_step walks both agents through the whole settle path", async () => {
  const servers = await fakeServers();
  try {
    const w = world(servers);

    // Nothing yet: alice is told to propose only when a goal says so.
    assert.equal((await w.alice.nextStep()).action, "none");
    const start = await w.alice.nextStep({ goal: { acceptor: "bob@y.test" } });
    assert.equal(start.action, "propose");

    const p = await w.alice.propose(TERMS, { acceptor: "bob@y.test" });

    // Bob holds no terms yet, so he is sent to the relay, not to accept.
    let s = await w.bob.nextStep();
    assert.equal(s.action, "check_messages", s.why);
    // Alice is waiting, not acting.
    assert.equal((await w.alice.nextStep()).action, "none");

    // Terms arrive off-chain; now bob accepts.
    await w.bob.accept(p.id, p.terms);
    s = await w.alice.nextStep();
    assert.equal(s.action, "fund", s.why);
    assert.equal(s.agreement, p.id);
    // Bob has nothing to do until it is funded.
    assert.equal((await w.bob.nextStep()).action, "none");

    await w.alice.fund(p.id);

    // Alice owes the input first; bob's deliverable is blocked on it.
    s = await w.alice.nextStep();
    assert.equal(s.action, "handoff", s.why);
    assert.equal(s.obligation, 0);
    assert.equal((await w.bob.nextStep()).action, "none", "bob's deliverable is blocked until the input lands");

    const h = await w.alice.handoff(p.id, { content: "ten papers", obligation: 0 });

    // Now bob must acknowledge, not deliver.
    s = await w.bob.nextStep();
    assert.equal(s.action, "acknowledge_receipt", s.why);
    await w.bob.acknowledgeReceipt(p.id, { handoff_id: h.evidence_id, content: "ten papers" });

    // With the input met, bob's deliverable unblocks.
    s = await w.bob.nextStep();
    assert.equal(s.action, "deliver", s.why);
    assert.equal(s.obligation, 1);
    await w.bob.deliver(p.id, { content: "summaries" });

    // Alice settles as payer; bob waits.
    assert.equal((await w.bob.nextStep()).action, "none");
    s = await w.alice.nextStep();
    assert.equal(s.action, "settle", s.why);
    await w.alice.settle(p.id);

    // Finished: both idle, and a goal does not restart a settled agreement
    // unless there is genuinely nothing open.
    assert.equal((await w.alice.nextStep()).action, "none");
    assert.equal((await w.bob.nextStep()).action, "none");
  } finally {
    await servers.close();
  }
});

test("the accept step carries enough to judge the work before accepting", async () => {
  const servers = await fakeServers();
  try {
    const w = world(servers);
    const p = await w.alice.propose(TERMS, { acceptor: "bob@y.test" });
    await w.bob.accept(p.id, p.terms).catch(() => {});
    // Re-read from a fresh bob who holds the terms but has not accepted.
    const w2 = world(servers);
    const p2 = await w2.alice.propose(TERMS, { acceptor: "bob@y.test" });
    w2.bob.terms.set(p2.id, { terms: p2.terms, proposal: p2.proposal });

    const s = await w2.bob.nextStep();
    assert.equal(s.action, "accept");
    const r = s.review;
    assert.equal(r.from, "alice@x.test");
    assert.equal(r.title, "Summarize 10 papers");
    // Only MY obligations, so a responder can ask "can I actually do this?"
    assert.deepEqual(r.i_must.map((o) => o.what), ["deliver summaries"]);
    assert.equal(r.i_am_paid.amount, "50.00");
    assert.equal(r.i_pay, null, "bob is the payee, not the payer");
    assert.ok(r.adjudicator, "the adjudicator is named before accepting");
  } finally {
    await servers.close();
  }
});

test("next_step sends the disputed party to respond", async () => {
  const servers = await fakeServers();
  try {
    const w = world(servers);
    const p = await w.alice.propose(TERMS, { acceptor: "bob@y.test" });
    await w.bob.accept(p.id, p.terms);
    await w.alice.fund(p.id);
    const h = await w.alice.handoff(p.id, { content: "ten papers", obligation: 0 });
    await w.bob.acknowledgeReceipt(p.id, { handoff_id: h.evidence_id, content: "ten papers" });
    await w.bob.deliver(p.id, { content: "summaries" });
    await w.alice.dispute(p.id, { note: "only five of ten" });

    const s = await w.bob.nextStep();
    assert.equal(s.action, "respond", s.why);
    // The disputer waits for the judge rather than acting again.
    assert.equal((await w.alice.nextStep()).action, "none");
  } finally {
    await servers.close();
  }
});
