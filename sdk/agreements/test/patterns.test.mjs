import test from "node:test";
import assert from "node:assert/strict";
import { Agreements, MemoryChain, deriveObligations, contentCommitment, generateSealingKey } from "../index.mjs";
import { FakeAgent, fakeVerify, fakeServers } from "./fakes.mjs";

const REPO = "sbo+raw://avail:turing:506/";

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

// Deadlines are BLOCK HEIGHTS. These are far enough ahead that nothing in the
// test can reach them — the test is about state transitions, not lateness.
const TERMS = {
  title: "Summarize 10 papers",
  obligations: [
    { by: "alice@x.test", what: "hand over the ten papers", deadline_block: 1_000_000, kind: "input" },
    { by: "bob@y.test", what: "deliver the summaries", deadline_block: 2_000_000, kind: "deliverable", depends_on: 0 },
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
    assert.ok(obs[1].response_due_block > 0, "the recipient is now on a clock");

    await assert.rejects(w.bob.sdk.acknowledgeReceipt(p.id, { handoff_id: "ev_nope", content: papers }), /no_handoff/);
    await assert.rejects(w.bob.sdk.acknowledgeReceipt(p.id, { handoff_id: h.evidence_id }), /bad_request/);
  } finally {
    await servers.close();
  }
});

// All times below are BLOCK HEIGHTS, not seconds (browserid-pay-e1ou). The
// response window converts at ~20s a block, so "PT1H" is 180 blocks.
test("deriveObligations: lateness, sliding deadlines, dispute rights", () => {
  const obligations = [
    { by: "a", what: "inputs", deadline_block: 1000, kind: "input" },
    { by: "b", what: "work", deadline_block: 2000, kind: "deliverable", depends_on: 0 },
    { by: "a", what: "pay", deadline_block: 1000, kind: "payment" },
  ];
  const parties = { proposer: "a", acceptor: "b" };
  // Nothing happened, past every deadline: a is late on inputs; b is blocked, not late; payment late.
  let s = deriveObligations(obligations, { evidence: [], lock: false, head: 3000, parties });
  assert.deepEqual(s.map((o) => o.state), ["late", "blocked", "late"]);
  assert.equal(s[0].dispute_right, "b", "the party let down holds the dispute right");
  assert.equal(s[1].dispute_right, null);
  // Inputs received in block 1500 (500 blocks late): b's deadline slides to 2500.
  const ev = [
    { id: "h1", kind: "handoff", by: "a", block: 1400, obligation: 0, commitment: "sha256:x" },
    { id: "r1", kind: "receipt", by: "b", block: 1500, handoff: "h1", ok: true, obligation: 0 },
  ];
  s = deriveObligations(obligations, { evidence: ev, lock: true, head: 2200, parties });
  assert.deepEqual(s.map((o) => o.state), ["met", "pending", "met"]);
  assert.equal(s[1].effective_deadline_block, 2500);
  s = deriveObligations(obligations, { evidence: ev, lock: true, head: 2600, parties });
  assert.equal(s[1].state, "late");
  assert.equal(s[1].dispute_right, "a");
  // A contested receipt.
  s = deriveObligations(obligations, { evidence: [ev[0], { ...ev[1], id: "r2", ok: false, reason: "wrong files" }], lock: true, head: 1600, parties });
  assert.equal(s[0].state, "contested");
  assert.equal(s[0].dispute_right, "b");
  // Untyped obligations are still deliverables, and a delivery no longer
  // settles one outright. The response window can only pull the clock IN: here
  // the obligation's own deadline (block 10) is far sooner than delivery + 180
  // blocks, so the deadline governs and head 20 is already past it.
  const one = [{ by: "b", what: "x", deadline_block: 10 }];
  const delivered = [{ id: "d", kind: "delivery", by: "b", block: 5 }];
  s = deriveObligations(one, { evidence: delivered, head: 20, responseWindow: "PT1H" });
  assert.equal(s[0].kind, "deliverable");
  assert.equal(s[0].response_due_block, 10, "the agreement deadline wins over a later response window");
  assert.equal(s[0].state, "unanswered");
  // Asked before that deadline, it is simply awaiting an answer.
  s = deriveObligations(one, { evidence: delivered, head: 8, responseWindow: "PT1H" });
  assert.equal(s[0].state, "delivered");
});

test("silence past the clock resolves nothing and pays nobody", () => {
  const obligations = [{ by: "b", what: "the work", kind: "deliverable", deadline_block: 21_600 }];
  const parties = { proposer: "a", acceptor: "b" };
  const opts = (head, evidence) => ({ evidence, head, parties, responseWindow: "PT1H" });
  const deliver = { id: "d1", kind: "delivery", by: "b", block: 1000 };

  // PT1H is 180 blocks, so the clock is block 1180 — well inside the deadline.
  let s = deriveObligations(obligations, opts(1100, [deliver]));
  assert.equal(s[0].state, "delivered");
  assert.equal(s[0].response_due_block, 1180);

  // Past it with no answer: UNANSWERED, not met. Silence is missing
  // information, not a finished job, and it pays nobody.
  s = deriveObligations(obligations, opts(9999, [deliver]));
  assert.equal(s[0].state, "unanswered");
  assert.equal(s[0].met_at_block, null);
  assert.equal(s[0].dispute_right, null, "nobody is at fault yet; either party may raise it");

  // An acknowledgement with no `ok` is an acceptance, as it always was.
  s = deriveObligations(obligations, opts(1160, [deliver, { id: "a2", kind: "acknowledgement", by: "a", block: 1150 }]));
  assert.equal(s[0].state, "met");
  assert.equal(s[0].met_at_block, 1150);
});

test("a re-delivery only re-anchors the clock when it answers not_met", () => {
  // The anti-stall property in one place. `not_received` is the unfalsifiable
  // claim, so it must buy no time; `not_met` is about work both parties can
  // see, and it moves the clock only if the DELIVERER concedes by trying again.
  const obligations = [{ by: "b", what: "the work", kind: "deliverable", deadline_block: 21_600 }];
  const parties = { proposer: "a", acceptor: "b" };
  const opts = (head, evidence) => ({ evidence, head, parties, responseWindow: "PT1H" });
  const del = (id, block) => ({ id, kind: "delivery", by: "b", block });
  const refuse = (id, block, reason) => ({ id, kind: "acknowledgement", by: "a", block, ok: false, reason });

  // Delivered early: the clock is pulled IN, so finishing early means being
  // paid early rather than waiting out the deadline. That is what the response
  // window is FOR.
  let s = deriveObligations(obligations, opts(800, [del("d1", 300)]));
  assert.equal(s[0].response_due_block, 480);

  // "I never got it", then a re-delivery: the clock does not move, and
  // repeating it cannot stall because it never buys a minute.
  s = deriveObligations(obligations, opts(800, [del("d1", 300), refuse("a1", 400, "not_received"), del("d2", 500)]));
  assert.equal(s[0].response_due_block, 480, "not_received re-anchors nothing");
  s = deriveObligations(obligations, opts(800, [del("d1", 300), refuse("a1", 400, "not_received"), del("d2", 500),
    refuse("a2", 600, "not_received"), del("d3", 700)]));
  assert.equal(s[0].response_due_block, 480, "nor does repeating it");

  // "I got it and it is wrong", then a re-delivery: the deliverer conceded, so
  // the clock re-anchors to the new attempt.
  s = deriveObligations(obligations, opts(800, [del("d1", 300), refuse("a1", 400, "not_met"), del("d2", 500)]));
  assert.equal(s[0].response_due_block, 680, "not_met + a retry re-anchors");

  // A complaint with no retry leaves it contested with the complainant.
  s = deriveObligations(obligations, opts(800, [del("d1", 300), refuse("a1", 400, "not_met")]));
  assert.equal(s[0].state, "contested");
  assert.equal(s[0].dispute_right, "a");

  // And re-anchoring can never push PAST the agreement's own deadline.
  s = deriveObligations(obligations, opts(21_550, [del("d1", 300), refuse("a1", 400, "not_met"), del("d2", 21_500)]));
  assert.equal(s[0].response_due_block, 21_600, "the agreement deadline still wins");
});

test("a writer-supplied timestamp cannot change lateness", () => {
  // The regression that started all of this (browserid-pay-e1ou): every
  // obligation decision used to come from `at`, a field the interested party
  // wrote itself and nothing validated. Deliver late, claim an earlier `at`,
  // and both the SDK and the adjudicator agreed it was on time.
  //
  // `at` is gone from what the SDK writes, but a counterparty controls its own
  // payloads and can put anything in them. So the guard is not "we stopped
  // writing it" — it is that the value is INERT even when present.
  const obligations = [{ by: "b", what: "the work", kind: "deliverable", deadline_block: 100 }];
  const parties = { proposer: "a", acceptor: "b" };
  const honest = [{ id: "d1", kind: "delivery", by: "b", block: 500 }];
  const lying = [{ id: "d1", kind: "delivery", by: "b", block: 500, at: 1 }];

  const a = deriveObligations(obligations, { evidence: honest, head: 600, parties });
  const b = deriveObligations(obligations, { evidence: lying, head: 600, parties });
  assert.deepEqual(a, b, "a payload timestamp changes nothing");
  assert.equal(b[0].state, "unanswered", "delivered long past the deadline, and saying otherwise does not help");

  // Nor can a backdated `at` win the anchor: ordering is by block alone.
  const ordered = deriveObligations(obligations, {
    evidence: [
      { id: "d1", kind: "delivery", by: "b", block: 10, at: 9_999 },
      { id: "a1", kind: "acknowledgement", by: "a", block: 11, at: 1, ok: false, reason: "not_met" },
      { id: "d2", kind: "delivery", by: "b", block: 12, at: 2 },
    ],
    head: 13, parties, responseWindow: "PT1H",
  });
  assert.equal(ordered[0].response_due_block, 100, "re-anchor to block 12 + 180, capped by the deadline");
});

test("an unconfirmed write has not landed and meets nothing", () => {
  // A write still in the mempool has no block. It was submitted, which is all
  // that can be said — treating a missing height as 0 would make it the
  // earliest thing on the chain and settle obligations it never touched.
  const obligations = [{ by: "a", what: "inputs", kind: "input", deadline_block: 100 }];
  const pending = [
    { id: "h1", kind: "handoff", by: "a", block: 10, obligation: 0, commitment: "sha256:x" },
    { id: "r1", kind: "receipt", by: "b", handoff: "h1", ok: true, obligation: 0 },
  ];
  const s = deriveObligations(obligations, { evidence: pending, head: 50, parties: { proposer: "a", acceptor: "b" } });
  assert.notEqual(s[0].state, "met", "a receipt that has not landed does not meet the obligation");
  assert.equal(s[0].met_at_block, null);
});

test("one delivery answers ONE milestone, not every deliverable the party owes", () => {
  // Carl's live case: two deliverables, one delivery. It used to mark BOTH met
  // — including the final one — leaving the payer a choice between paying for
  // work not yet done and posting a `not_met` that was not true.
  const obligations = [
    { by: "carl", what: "three concepts", deadline_block: 2000, kind: "deliverable" },
    { by: "carl", what: "final mark", deadline_block: 4000, kind: "deliverable" },
  ];
  const parties = { proposer: "alice", acceptor: "carl" };
  const first = [{ id: "d1", kind: "delivery", by: "carl", block: 1900, commitment: "sha256:a" }];

  let s = deriveObligations(obligations, { evidence: first, lock: true, head: 1950, parties });
  assert.equal(s[0].state, "delivered", "the first milestone is answered");
  assert.equal(s[1].state, "pending", "the final mark is NOT delivered by someone else's commitment");

  // Accepting milestone 1 must not carry milestone 2 with it.
  const acked = [...first, { id: "e1", kind: "acknowledgement", by: "alice", block: 1960, ok: true }];
  s = deriveObligations(obligations, { evidence: acked, lock: true, head: 1970, parties });
  assert.equal(s[0].state, "met");
  assert.equal(s[1].state, "pending", "the final mark is still owed after the first is accepted");

  // Tagging is what the ledger reads; the note is prose.
  const tagged = [...acked, { id: "d2", kind: "delivery", by: "carl", block: 3900, obligation: 1, commitment: "sha256:b" }];
  s = deriveObligations(obligations, { evidence: tagged, lock: true, head: 3950, parties });
  assert.deepEqual(s.map((o) => o.state), ["met", "delivered"]);

  // A single-deliverable agreement is untouched: no tag needed, no change.
  const one = [{ by: "carl", what: "the logo", deadline_block: 2000, kind: "deliverable" }];
  s = deriveObligations(one, { evidence: first, lock: true, head: 1950, parties });
  assert.equal(s[0].state, "delivered");
});
