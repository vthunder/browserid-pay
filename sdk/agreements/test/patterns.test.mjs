import test from "node:test";
import assert from "node:assert/strict";
import { Agreements, MemoryChain, deriveObligations, contentCommitment, generateSealingKey } from "../index.mjs";
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
    return { agent, sdk: new Agreements({ agent, chain, repo: REPO, verify: fakeVerify, obtainWarrants: agent.obtainAll, http: fetch }) };
  };
  return { chain, alice: mk("alice@x.test"), bob: mk("bob@y.test") };
}

const NOW = Math.floor(Date.now() / 1000);
const TERMS = {
  title: "Summarize 10 papers",
  obligations: [
    { by: "alice@x.test", what: "hand over the ten papers", deadline: NOW + 86400, kind: "input" },
    { by: "bob@y.test", what: "deliver the summaries", deadline: NOW + 7 * 86400, kind: "deliverable", depends_on: 0 },
  ],
  price: { amount: "50.00", currency: "USD" },
  evidence: "handoff + receipt for the papers; a delivery commitment for the summaries",
};

test("handoff → receipt verifies the hash; obligations move blocked → pending → met", async () => {
  const servers = await fakeServers();
  try {
    const w = world(servers);
    const p = await w.alice.sdk.propose(TERMS, { acceptor: "bob@y.test" });
    await w.bob.sdk.accept(p.id, p.terms);

    let obs = await w.bob.sdk.obligations(p.id);
    assert.deepEqual(obs.map((o) => o.state), ["pending", "blocked"], "bob is blocked until alice hands off");

    const papers = ["Attention Is All You Need", "BERT", "GPT-3"];
    const h = await w.alice.sdk.handoff(p.id, { content: papers, obligation: 0, note: "the papers" });
    assert.equal(h.evidence.kind, "handoff");
    assert.equal(h.evidence.commitment, contentCommitment(papers));
    assert.equal(h.evidence.manifest.count, 3);
    obs = await w.bob.sdk.obligations(p.id);
    assert.deepEqual(obs.map((o) => o.state), ["handed_off", "blocked"]);

    // Wrong content ⇒ contested with a reason; right content ⇒ ok.
    const bad = await w.bob.sdk.acknowledgeReceipt(p.id, { handoff_id: h.evidence_id, content: ["something else"] });
    assert.equal(bad.evidence.ok, false);
    assert.match(bad.evidence.reason, /hashes to sha256:/);
    const good = await w.bob.sdk.acknowledgeReceipt(p.id, { handoff_id: h.evidence_id, content: papers });
    assert.equal(good.evidence.ok, true);
    assert.equal(good.evidence.handoff, h.evidence_id);

    obs = await w.bob.sdk.obligations(p.id);
    assert.deepEqual(obs.map((o) => o.state), ["met", "pending"], "receipt met the input; bob's deliverable is now pending");
    await w.bob.sdk.deliver(p.id, { content: { summaries: 3 } });
    obs = await w.bob.sdk.obligations(p.id);
    // A delivery commitment starts alice's response window; it is not yet met.
    assert.deepEqual(obs.map((o) => o.state), ["met", "delivered"]);
    assert.ok(obs[1].response_due > 0, "the recipient is now on a clock");

    await assert.rejects(w.bob.sdk.acknowledgeReceipt(p.id, { handoff_id: "ev_nope", content: papers }), /no_handoff/);
    await assert.rejects(w.bob.sdk.acknowledgeReceipt(p.id, { handoff_id: h.evidence_id }), /bad_request/);
  } finally {
    await servers.close();
  }
});

test("deriveObligations: lateness, sliding deadlines, dispute rights", () => {
  const obligations = [
    { by: "a", what: "inputs", deadline: 1000, kind: "input" },
    { by: "b", what: "work", deadline: 2000, kind: "deliverable", depends_on: 0 },
    { by: "a", what: "pay", deadline: 1000, kind: "payment" },
  ];
  const parties = { proposer: "a", acceptor: "b" };
  // Nothing happened, past every deadline: a is late on inputs; b is blocked, not late; payment late.
  let s = deriveObligations(obligations, { evidence: [], lock: false, now: 3000, parties });
  assert.deepEqual(s.map((o) => o.state), ["late", "blocked", "late"]);
  assert.equal(s[0].dispute_right, "b", "the party let down holds the dispute right");
  assert.equal(s[1].dispute_right, null);
  // Inputs handed off at 1500 (500 late) and received: b's deadline slides to 2500.
  const ev = [
    { id: "h1", kind: "handoff", by: "a", at: 1400, obligation: 0, commitment: "sha256:x" },
    { id: "r1", kind: "receipt", by: "b", at: 1500, handoff: "h1", ok: true, obligation: 0 },
  ];
  s = deriveObligations(obligations, { evidence: ev, lock: true, now: 2200, parties });
  assert.deepEqual(s.map((o) => o.state), ["met", "pending", "met"]);
  assert.equal(s[1].effective_deadline, 2500);
  s = deriveObligations(obligations, { evidence: ev, lock: true, now: 2600, parties });
  assert.equal(s[1].state, "late");
  assert.equal(s[1].dispute_right, "a");
  // A contested receipt.
  s = deriveObligations(obligations, { evidence: [ev[0], { ...ev[1], id: "r2", ok: false, reason: "wrong files" }], lock: true, now: 1600, parties });
  assert.equal(s[0].state, "contested");
  assert.equal(s[0].dispute_right, "b");
  // Untyped obligations are still deliverables, and a delivery now starts the
  // recipient's response window rather than settling the obligation outright.
  const one = [{ by: "b", what: "x", deadline: 10 }];
  const delivered = [{ id: "d", kind: "delivery", by: "b", at: 5 }];
  s = deriveObligations(one, { evidence: delivered, now: 20, responseWindow: "PT1H" });
  assert.equal(s[0].kind, "deliverable");
  assert.equal(s[0].state, "delivered");
  assert.equal(s[0].response_due, 5 + 3600);
});

test("a delivery starts the recipient's clock; silence is acceptance, a refusal is not", () => {
  const obligations = [{ by: "b", what: "the work", kind: "deliverable", deadline: 10_000 }];
  const parties = { proposer: "a", acceptor: "b" };
  const deliver = { id: "d1", kind: "delivery", by: "b", at: 1000 };
  const opts = (now, evidence) => ({ evidence, now, parties, responseWindow: "PT1H" });

  // Inside the window: awaiting a response, not met.
  let s = deriveObligations(obligations, opts(1500, [deliver]));
  assert.equal(s[0].state, "delivered");
  assert.equal(s[0].response_due, 4600);

  // Past it with no answer: deemed accepted, dated at the deadline rather than
  // at "now", so the moment it became met does not depend on when you ask.
  s = deriveObligations(obligations, opts(9999, [deliver]));
  assert.equal(s[0].state, "met");
  assert.equal(s[0].deemed_accepted, true);
  assert.equal(s[0].met_at, 4600);

  // THE SAFETY PROPERTY: an explicit refusal must stop the clock, or the
  // deemed-acceptance rule would pay a withholder who simply waited.
  const refused = [deliver, { id: "a1", kind: "acknowledgement", by: "a", at: 1200, ok: false, reason: "not_received" }];
  s = deriveObligations(obligations, opts(9999, refused));
  assert.equal(s[0].state, "contested");
  assert.equal(s[0].contest_reason, "not_received");
  assert.equal(s[0].deemed_accepted, undefined);
  assert.equal(s[0].dispute_right, "a", "the party who refused holds it");

  // A cure is simply a later delivery: it becomes the latest and the clock
  // restarts, so the refusal no longer stands.
  const cured = [...refused, { id: "d2", kind: "delivery", by: "b", at: 2000 }];
  s = deriveObligations(obligations, opts(2100, cured));
  assert.equal(s[0].state, "delivered");
  assert.equal(s[0].response_due, 2000 + 3600);

  // And an acknowledgement with no `ok` is an acceptance, as it always was.
  const legacy = [deliver, { id: "a2", kind: "acknowledgement", by: "a", at: 1200 }];
  s = deriveObligations(obligations, opts(1300, legacy));
  assert.equal(s[0].state, "met");
  assert.equal(s[0].met_at, 1200);
});

test("repeated not_received cannot stall for ever: the absolute clock does not reset", () => {
  const obligations = [{ by: "b", what: "the work", kind: "deliverable", deadline: 10_000 }];
  const parties = { proposer: "a", acceptor: "b" };
  // Response window 1h, absolute limit 3h from the FIRST delivery.
  const opts = (now, evidence) => ({ evidence, now, parties, responseWindow: "PT1H", resolutionWindow: "PT3H" });
  const deliver = (id, at) => ({ id, kind: "delivery", by: "b", at });
  const refuse = (id, at) => ({ id, kind: "acknowledgement", by: "a", at, ok: false, reason: "not_received" });

  // Cure #1, refused again: still contested, still inside the absolute limit.
  let ev = [deliver("d1", 1000), refuse("a1", 1100), deliver("d2", 2000), refuse("a2", 2100)];
  let s = deriveObligations(obligations, opts(2200, ev));
  assert.equal(s[0].state, "contested");
  assert.equal(s[0].absolute_due, 1000 + 3 * 3600, "measured from the FIRST delivery, not the latest");

  // Cure #3 lands after the absolute limit. Under the old rule each cure reset
  // the clock and this could run for ever; now the argument is out of time.
  ev = [...ev, deliver("d3", 12_000), refuse("a3", 12_100)];
  s = deriveObligations(obligations, opts(12_200, ev));
  assert.equal(s[0].state, "stalled");
  assert.equal(s[0].dispute_right, "b", "the deliverer escalates — nothing else will resolve it");

  // A quality complaint is different: it ACKNOWLEDGES receipt, so the arrival
  // clock is behind us and this stays an ordinary contest however long it runs.
  const quality = [deliver("d1", 1000), { id: "q", kind: "acknowledgement", by: "a", at: 1100, ok: false, reason: "not_met" }];
  s = deriveObligations(obligations, opts(99_000, quality));
  assert.equal(s[0].state, "contested");
  assert.equal(s[0].contest_reason, "not_met");
  assert.equal(s[0].dispute_right, "a");

  // And silence is still acceptance, whatever the absolute clock says — this
  // limit bounds an argument, it does not punish a quiet recipient.
  s = deriveObligations(obligations, opts(99_000, [deliver("d1", 1000)]));
  assert.equal(s[0].state, "met");
  assert.equal(s[0].deemed_accepted, true);
});
