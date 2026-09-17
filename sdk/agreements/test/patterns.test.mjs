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
  // Untyped obligations are still deliverables, and a delivery no longer
  // settles one outright. The response window can only pull the clock IN: here
  // the obligation's own deadline (10) is far sooner than delivery + 1h, so the
  // deadline governs and `now: 20` is already past it.
  const one = [{ by: "b", what: "x", deadline: 10 }];
  const delivered = [{ id: "d", kind: "delivery", by: "b", at: 5 }];
  s = deriveObligations(one, { evidence: delivered, now: 20, responseWindow: "PT1H" });
  assert.equal(s[0].kind, "deliverable");
  assert.equal(s[0].response_due, 10, "the agreement deadline wins over a later response window");
  assert.equal(s[0].state, "unanswered");
  // Asked before that deadline, it is simply awaiting an answer.
  s = deriveObligations(one, { evidence: delivered, now: 8, responseWindow: "PT1H" });
  assert.equal(s[0].state, "delivered");
});

test("silence past the clock resolves nothing and pays nobody", () => {
  const obligations = [{ by: "b", what: "the work", kind: "deliverable", deadline: 21_600 }];
  const parties = { proposer: "a", acceptor: "b" };
  const opts = (now, evidence) => ({ evidence, now, parties, responseWindow: "PT1H" });
  const deliver = { id: "d1", kind: "delivery", by: "b", at: 1000 };

  let s = deriveObligations(obligations, opts(1500, [deliver]));
  assert.equal(s[0].state, "delivered");
  assert.equal(s[0].response_due, 4600);

  // Past it with no answer: UNANSWERED, not met. Silence is missing
  // information, not a finished job, and it pays nobody.
  s = deriveObligations(obligations, opts(9999, [deliver]));
  assert.equal(s[0].state, "unanswered");
  assert.equal(s[0].met_at, null);
  assert.equal(s[0].dispute_right, null, "nobody is at fault yet; either party may raise it");

  // An acknowledgement with no `ok` is an acceptance, as it always was.
  s = deriveObligations(obligations, opts(1300, [deliver, { id: "a2", kind: "acknowledgement", by: "a", at: 1200 }]));
  assert.equal(s[0].state, "met");
  assert.equal(s[0].met_at, 1200);
});

test("a re-delivery only re-anchors the clock when it answers not_met", () => {
  // The anti-stall property in one place. `not_received` is the unfalsifiable
  // claim, so it must buy no time; `not_met` is about work both parties can
  // see, and it moves the clock only if the DELIVERER concedes by trying again.
  const obligations = [{ by: "b", what: "the work", kind: "deliverable", deadline: 21_600 }];
  const parties = { proposer: "a", acceptor: "b" };
  const opts = (now, evidence) => ({ evidence, now, parties, responseWindow: "PT1H" });
  const del = (id, at) => ({ id, kind: "delivery", by: "b", at });
  const refuse = (id, at, reason) => ({ id, kind: "acknowledgement", by: "a", at, ok: false, reason });

  // Delivered early: the clock is pulled IN, so finishing early means being
  // paid early rather than waiting out the deadline. That is what the response
  // window is FOR.
  let s = deriveObligations(obligations, opts(1000, [del("d1", 300)]));
  assert.equal(s[0].response_due, 3900);

  // "I never got it", then a re-delivery: the clock does not move, and
  // repeating it cannot stall because it never buys a minute.
  s = deriveObligations(obligations, opts(1000, [del("d1", 300), refuse("a1", 400, "not_received"), del("d2", 500)]));
  assert.equal(s[0].response_due, 3900, "not_received re-anchors nothing");
  s = deriveObligations(obligations, opts(1000, [del("d1", 300), refuse("a1", 400, "not_received"), del("d2", 500),
    refuse("a2", 600, "not_received"), del("d3", 700)]));
  assert.equal(s[0].response_due, 3900, "nor does repeating it");

  // "I got it and it is wrong", then a re-delivery: the deliverer conceded, so
  // the clock re-anchors to the new attempt.
  s = deriveObligations(obligations, opts(1000, [del("d1", 300), refuse("a1", 400, "not_met"), del("d2", 500)]));
  assert.equal(s[0].response_due, 4100, "not_met + a retry re-anchors");

  // A complaint with no retry leaves it contested with the complainant.
  s = deriveObligations(obligations, opts(1000, [del("d1", 300), refuse("a1", 400, "not_met")]));
  assert.equal(s[0].state, "contested");
  assert.equal(s[0].dispute_right, "a");

  // And re-anchoring can never push PAST the agreement's own deadline.
  s = deriveObligations(obligations, opts(21_500, [del("d1", 300), refuse("a1", 400, "not_met"), del("d2", 21_000)]));
  assert.equal(s[0].response_due, 21_600, "the agreement deadline still wins");
});
