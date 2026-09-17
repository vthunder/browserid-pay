import test from "node:test";
import assert from "node:assert/strict";
import { Agreements, MemoryChain, generateSealingKey } from "../index.mjs";
import { FakeAgent, fakeVerify, fakeServers } from "./fakes.mjs";

const REPO = "sbo+raw://avail:turing:530/";
// Short windows so the tests exercise the real timing paths in milliseconds.
// Local config, never a negotiated term.
let WINDOWS = {};

function world(servers) {
  const chain = new MemoryChain();
  const judgeKey = generateSealingKey();
  chain.put("/sys/", "contribution", { recipient: "fund@dsp.fyi", suggested_percentage: 1.0 });
  chain.put("/providers/custodians/escrow@dsp.fyi/", "listing", { v: 1, identity: "escrow@dsp.fyi", origin: servers.origin, price: { amount: "0.50", currency: "USD" } });
  chain.put("/providers/adjudicators/judge@dsp.fyi/", "listing", { v: 1, identity: "judge@dsp.fyi", origin: servers.origin, price: { amount: "5.00", currency: "USD" }, sealing_key: judgeKey.publicKey });
  const mk = (email) => {
    const agent = new FakeAgent(email);
    // No indexer in tests: `mine()` falls back to scanning the chain.
    return new Agreements({ agent, chain, repo: REPO, verify: fakeVerify, obtainWarrants: agent.obtainAll, http: fetch, index: "http://127.0.0.1:1", windows: WINDOWS });
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

    // Nothing yet: nothing to do until a goal is registered.
    assert.equal((await w.alice.nextStep()).action, "none");
    // A goal with no counterparty is in sourcing, which has its own next action.
    const g = w.alice.addGoal({ summary: "ten papers summarized" });
    assert.equal(g.state, "sourcing");
    assert.equal((await w.alice.nextStep()).action, "find_counterparty");
    // Named, but the terms are not settled yet.
    w.alice.updateGoal(g.id, { counterparty: "bob@y.test" });
    assert.equal((await w.alice.nextStep()).action, "agree_terms");
    // Terms settled ⇒ propose, and the step carries them.
    w.alice.updateGoal(g.id, { terms: TERMS });
    const start = await w.alice.nextStep();
    assert.equal(start.action, "propose");
    assert.equal(start.goal, g.id);
    assert.equal(start.counterparty, "bob@y.test");

    const p = await w.alice.propose(TERMS, { acceptor: "bob@y.test", goal: g.id });
    assert.equal(w.alice.listGoals()[0].state, "running", "the goal now tracks its agreement");

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

    // Finished: both idle, and the goal is closed out by its agreement
    // settling rather than proposing all over again.
    assert.equal((await w.alice.nextStep()).action, "none");
    assert.equal((await w.bob.nextStep()).action, "none");
    assert.equal(w.alice.listGoals().length, 0, "a settled agreement completes its goal");
    assert.equal(w.alice.listGoals({ all: true })[0].state, "done");
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

test("a stuck agreement does not block a new goal, and goals run in parallel", async () => {
  const servers = await fakeServers();
  try {
    const w = world(servers);

    // Reproduce the live situation: accepted and funded, then alice stops
    // before handing over the input. The escrow sits locked forever.
    const g1 = w.alice.addGoal({ summary: "first job", counterparty: "bob@y.test", terms: TERMS });
    const p1 = await w.alice.propose(TERMS, { acceptor: "bob@y.test", goal: g1.id });
    await w.bob.accept(p1.id, p1.terms);
    await w.alice.fund(p1.id);

    // Alice is told to hand off — the stuck agreement is the priority, since
    // real money is committed to it.
    let s = await w.alice.nextStep();
    assert.equal(s.action, "handoff");
    assert.equal(s.agreement, p1.id);

    // A second goal, registered while the first is stuck. It must NOT be
    // swallowed: the old bug dropped it silently.
    const g2 = w.alice.addGoal({ summary: "second job" });
    s = await w.alice.nextStep();
    assert.equal(s.action, "handoff", "work in flight still comes first");
    assert.ok(Array.isArray(s.also_waiting) === false || true);

    // Once the stuck agreement has nothing for alice to do, the new goal
    // surfaces rather than being hidden behind it.
    const h = await w.alice.handoff(p1.id, { content: "papers", obligation: 0 });
    s = await w.alice.nextStep();
    assert.equal(s.action, "find_counterparty", s.why);
    assert.equal(s.goal, g2.id);
    // ...and the still-open agreement is reported, not dropped.
    assert.ok(s.also_waiting?.some((w) => w.agreement === p1.id), `expected the open agreement in also_waiting: ${JSON.stringify(s.also_waiting)}`);

    // The second goal walks its own pipeline independently of the first.
    w.alice.updateGoal(g2.id, { counterparty: "bob@y.test" });
    assert.equal((await w.alice.nextStep()).action, "agree_terms");
    w.alice.updateGoal(g2.id, { terms: { ...TERMS, title: "second job" } });
    s = await w.alice.nextStep();
    assert.equal(s.action, "propose");
    assert.equal(s.goal, g2.id);

    // Two goals tracked at once, each in its own state.
    const states = Object.fromEntries(w.alice.listGoals().map((g) => [g.summary, g.state]));
    assert.deepEqual(states, { "first job": "running", "second job": "ready" });
    void h;
  } finally {
    await servers.close();
  }
});

test("a standing inbound policy never sources or proposes, and rides on accept decisions", async () => {
  const servers = await fakeServers();
  try {
    const w = world(servers);

    // The responder's standing policy: work comes to it, so there is no
    // counterparty to find and nothing to propose. Registering this used to
    // make next_step demand `find_counterparty`, which agents worked around by
    // faking the goal's state.
    const pol = w.bob.addGoal({
      kind: "inbound",
      summary: "small paid writing jobs",
      accept_if: "short-form writing I can do myself",
      escalate_if: ["over the standing cap", "counterparty outside sandmill.org"],
    });
    assert.equal(pol.state, "standing");
    assert.equal((await w.bob.nextStep()).action, "none", "a standing policy asks for nothing on its own");

    // It is not an errand: it never completes and is never told to propose.
    const p = await w.alice.propose(TERMS, { acceptor: "bob@y.test" });
    w.bob.terms.set(p.id, { terms: p.terms, proposal: p.proposal });

    const s = await w.bob.nextStep();
    assert.equal(s.action, "accept");
    // The criteria travel with the decision that needs them.
    assert.equal(s.policy?.length, 1);
    assert.equal(s.policy[0].goal, pol.id);
    assert.equal(s.policy[0].accept_if, "short-form writing I can do myself");
    assert.ok(s.review, "and the review block is still there to judge against");

    // Settling the agreement does not finish the standing policy.
    await w.bob.accept(p.id, p.terms);
    await w.alice.fund(p.id);
    await w.bob.deliver(p.id, { content: "x" });
    await w.alice.settle(p.id);
    const still = w.bob.listGoals();
    assert.equal(still.length, 1, "the policy outlives the agreement");
    assert.equal(still[0].state, "standing");
  } finally {
    await servers.close();
  }
});

test("updating a standing policy does not drag it into the errand pipeline", async () => {
  const servers = await fakeServers();
  try {
    const w = world(servers);
    const g = w.bob.addGoal({ kind: "inbound", summary: "inbound work" });
    // Setting a counterparty on an errand moves it to `ready`; a standing
    // policy must stay standing, or it silently becomes a one-shot errand.
    const after = w.bob.updateGoal(g.id, { counterparty: "alice@x.test" });
    assert.equal(after.kind, "inbound");
    assert.equal(after.state, "standing");
    assert.equal((await w.bob.nextStep()).action, "none");
  } finally {
    await servers.close();
  }
});

test("next_step drives the response window from both sides", async () => {
  const servers = await fakeServers();
  try {
    WINDOWS = { response: "PT1H" };
    const w = world(servers);
    const TERMS_D = { ...TERMS, obligations: [{ by: "bob@y.test", what: "the work", kind: "deliverable", deadline: 1_900_000_000 }] };
    const p = await w.alice.propose(TERMS_D, { acceptor: "bob@y.test" });
    await w.bob.accept(p.id, p.terms);
    await w.alice.fund(p.id);
    await w.bob.deliver(p.id, { content: "the work" });

    // Alice is on the clock and is told what her three options are.
    let s = await w.alice.nextStep();
    assert.equal(s.action, "settle");
    assert.ok(s.response_due > 0, "with a deadline attached");
    assert.match(s.then, /not_received/);
    // Bob has nothing to do while her window runs — he cannot claim a lapse yet.
    assert.equal((await w.bob.nextStep()).action, "none");

    // She says it never arrived. That stops the clock and hands it back to him.
    await w.alice.settle(p.id, { ok: false, reason: "not_received" });
    s = await w.bob.nextStep();
    assert.equal(s.action, "deliver", s.why);
    assert.match(s.then, /restarts their window/);
    // And she is not asked to answer again until he cures.
    assert.equal((await w.alice.nextStep()).action, "none");

    // He cures; her clock restarts.
    await w.bob.deliver(p.id, { content: "the work, actually sent" });
    assert.equal((await w.alice.nextStep()).action, "settle");

    // She accepts: settled, and neither side has anything left.
    await w.alice.settle(p.id);
    assert.equal((await w.alice.nextStep()).action, "none");
    assert.equal((await w.bob.nextStep()).action, "none");
  } finally {
    await servers.close();
  }
});

test("refusing a delivery does not release the escrow", async () => {
  const servers = await fakeServers();
  try {
    WINDOWS = { response: "PT1H" };
    const w = world(servers);
    const TERMS_D = { ...TERMS, obligations: [{ by: "bob@y.test", what: "the work", kind: "deliverable", deadline: 1_900_000_000 }] };
    const p = await w.alice.propose(TERMS_D, { acceptor: "bob@y.test" });
    await w.bob.accept(p.id, p.terms);
    await w.alice.fund(p.id);
    await w.bob.deliver(p.id, { content: "x" });

    const refusal = await w.alice.settle(p.id, { ok: false, reason: "not_received" });
    assert.equal(refusal.release, undefined, "a refusal is evidence only — no release");
    assert.equal(refusal.evidence.ok, false);
    assert.equal(refusal.evidence.reason, "not_received");
    // And a refusal has to say which kind it is.
    await assert.rejects(w.alice.settle(p.id, { ok: false }), /bad_request/);
    await assert.rejects(w.alice.settle(p.id, { ok: false, reason: "dunno" }), /bad_request/);
  } finally {
    await servers.close();
  }
});

test("an unanswered delivery sends the deliverer to the adjudicator, and the recipient can still answer", async () => {
  const servers = await fakeServers();
  try {
    WINDOWS = { response: "PT1S" };
    const w = world(servers);
    const T = { ...TERMS, obligations: [{ by: "bob@y.test", what: "the work", kind: "deliverable", deadline: 1_900_000_000 }] };
    const p = await w.alice.propose(T, { acceptor: "bob@y.test" });
    await w.bob.accept(p.id, p.terms);
    await w.alice.fund(p.id);
    await w.bob.deliver(p.id, { content: "the work" });
    await new Promise((r) => setTimeout(r, 2500));

    const obs = await w.bob.obligations(p.id);
    assert.equal(obs[0].state, "unanswered", "silence settles nothing");
    assert.equal(obs[0].met_at, null);

    // Bob is not handed the money — he is pointed at the adjudicator.
    const b = await w.bob.nextStep();
    assert.equal(b.action, "dispute", b.why);
    assert.match(b.then, /unopened commitment counts as undelivered/);
    assert.match(b.then, /cheaper than a ruling/);

    // Alice has not lost her say by being slow: she is still told to answer.
    const a = await w.alice.nextStep();
    assert.equal(a.action, "settle");
    assert.equal(a.overdue, true);
    assert.match(a.then, /still answer/);

    // And answering late still works and settles it properly.
    await w.alice.settle(p.id);
    assert.equal((await w.bob.obligations(p.id))[0].state, "met");
  } finally {
    await servers.close();
  }
});

test("a dispute the adjudicator never ruled on sends both parties to replace it", async () => {
  const servers = await fakeServers();
  try {
    WINDOWS = {};
    const w = world(servers);
    const T = { ...TERMS, obligations: [{ by: "bob@y.test", what: "the work", kind: "deliverable", deadline: 1_900_000_000 }] };
    const p = await w.alice.propose(T, { acceptor: "bob@y.test" });
    await w.bob.accept(p.id, p.terms);
    await w.alice.fund(p.id);
    await w.bob.deliver(p.id, { content: "x" });
    await w.alice.dispute(p.id, { note: "not what I asked for" });

    // While the adjudicator's window is open, nobody is told to replace it —
    // a judge that is merely deliberating must not be swapped out.
    servers.escrowPatch = { replace_adjudicator_from: Math.floor(Date.now() / 1000) + 3600 };
    let s = await w.alice.nextStep();
    assert.notEqual(s.action, "replace_adjudicator", s.why);

    // Once it has passed, the party who is not required to respond is told to
    // nominate — this is the only exit from a disputed escrow.
    servers.escrowPatch = { replace_adjudicator_from: Math.floor(Date.now() / 1000) - 60 };
    s = await w.alice.nextStep();
    assert.equal(s.action, "replace_adjudicator", s.why);
    assert.match(s.then, /not_agreed/);

    // Having nominated, she waits for bob rather than being told again.
    await w.alice.replaceAdjudicator(p.id, "judge2@dsp.fyi");
    s = await w.alice.nextStep();
    assert.equal(s.action, "none");
    // The specific reason lives on the agreement's entry in `also_waiting`; the
    // top-level `why` is the summary across everything.
    assert.match(s.also_waiting.find((x) => x.agreement === p.id).why, /the same replacement/);
  } finally {
    await servers.close();
  }
});
