import test from "node:test";
import assert from "node:assert/strict";
import { Agreements, MemoryChain, contractAudience, totalOf, generateSealingKey, open, canonicalize } from "../index.mjs";
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
  return { chain, judgeKey, alice: mk("alice@x.test"), bob: mk("bob@y.test") };
}

const TERMS = {
  title: "Summarize 10 papers",
  obligations: [{ by: "bob@y.test", what: "deliver summaries", deadline: 1_900_000_000 }],
  price: { amount: "50.00", currency: "USD" },
  evidence: "a delivery commitment plus the summaries sent off-chain",
};

test("propose → accept → fund → deliver → settle", async () => {
  const servers = await fakeServers();
  try {
    const w = world(servers);
    const p = await w.alice.sdk.propose(TERMS, { acceptor: "bob@y.test" });
    assert.match(p.id, /^agr_/);
    assert.equal(p.terms.fees.custodian, "0.50");
    assert.equal(p.terms.fees.adjudicator, "5.00");
    assert.equal(p.terms.contribution.percentage, 1);
    assert.equal(totalOf(p.terms), 5000 + 50 + 500 + 50);
    assert.equal(p.proposal.commitment, p.commitment);
    assert.equal(p.proposal.terms, undefined, "commitment mode hides terms");
    assert.deepEqual(p.proposal.roles, { payer: "alice@x.test", payee: "bob@y.test" });
    // ONE consent card for alice's human, carrying every grant this agreement
    // needs: the contract, the standing ledger-write, and the judge's audience.
    assert.equal(w.alice.agent.cards.length, 1, "one card");
    const asks = w.alice.agent.cards[0];
    assert.equal(asks.length, 3);
    assert.deepEqual(asks.slice(0, 2).map((a) => a.audience), [contractAudience(p.id, REPO), REPO]);
    assert.deepEqual(asks[0].scopes, [{ scope: "contract:agreement", cap: { amount: "56.00", currency: "USD" }, counterparties: ["bob@y.test"] }]);
    assert.deepEqual(asks[1].scopes, ["action:post", "path:/agreements/**", "path:/alice@x.test/**"]);
    assert.deepEqual(asks[2].scopes, ["dispute"]);
    // On chain: the proposal object under /agreements/<id>/.
    const onChain = await w.chain.get(`/agreements/${p.id}/`, "proposal");
    assert.equal(onChain.payload.parties.proposer, "alice@x.test");
    assert.equal(onChain.headers["Content-Schema"], "agreement.proposal.v1");
    assert.match(onChain.headers["Public-Key"], /^ed25519:[0-9a-f]{64}$/);
    assert.ok(onChain.headers["Auth-Cert"].split("~").length === 4);
    // Sealed terms open with the judge's key to the canonical terms.
    assert.equal(open(p.sealed_terms, w.judgeKey.secretKey).toString(), canonicalize(p.terms));

    // Bob accepts with the terms alice sent off-chain.
    const a = await w.bob.sdk.accept(p.id, p.terms);
    assert.match(a.acceptance.proposal_hash, /^sha256:/);
    assert.equal((await w.bob.sdk.status(p.id)).stage, "binding");

    // Alice (payer) funds → custodian lock.
    const lock = await w.alice.sdk.fund(p.id);
    assert.equal(lock.state, "locked");
    const lockCall = servers.calls.find((c) => c.url === "/api/escrow/lock");
    assert.equal(lockCall.body.proposal.id, p.id);
    assert.equal(open(lockCall.body.sealed_terms, w.judgeKey.secretKey).toString(), canonicalize(p.terms));
    assert.equal(lockCall.body.presentation.split("~").length, 4);
    w.chain.put(`/agreements/${p.id}/`, "lock", { v: 1, id: p.id, locked_at: 1 }, "escrow@dsp.fyi");
    assert.equal((await w.alice.sdk.status(p.id)).stage, "active");

    // Bob delivers, alice settles.
    const d = await w.bob.sdk.deliver(p.id, { content: { summaries: ["…"] }, note: "done" });
    assert.match(d.evidence.commitment, /^sha256:/);
    assert.equal(d.evidence.content, undefined);
    assert.equal((await w.alice.sdk.status(p.id)).stage, "delivered");
    const s = await w.alice.sdk.settle(p.id, { note: "thanks" });
    assert.equal(s.release.state, "released");
    const st = await w.bob.sdk.status(p.id);
    assert.equal(st.stage, "settled");
    assert.deepEqual(st.evidence.map((e) => e.kind), ["delivery", "acknowledgement"]);
    assert.equal(st.escrow.state, "released");
  } finally { await servers.close(); }
});

test("dispute freezes escrow and files with the judge", async () => {
  const servers = await fakeServers();
  try {
    const w = world(servers);
    const p = await w.alice.sdk.propose(TERMS, { acceptor: "bob@y.test", payer: "bob@y.test" });
    assert.deepEqual(p.proposal.roles, { payer: "bob@y.test", payee: "alice@x.test" }, "payment runs either direction");
    await w.bob.sdk.accept(p.id, p.terms);
    await assert.rejects(w.alice.sdk.fund(p.id), /not_payer/);
    await w.bob.sdk.fund(p.id);
    const d = await w.alice.sdk.dispute(p.id, { note: "never paid" });
    assert.equal(d.frozen.state, "disputed");
    assert.equal(d.filed.filed, true);
    const filed = servers.calls.find((c) => c.url === "/api/dispute");
    assert.equal(filed.body.contract_id, p.id);
    assert.ok(filed.body.sealed_terms, "sealed terms reach the judge");
    assert.equal((await w.bob.sdk.status(p.id)).stage, "disputed");
  } finally { await servers.close(); }
});

test("accept refuses a bad commitment, a wrong party, an undersized cap", async () => {
  const servers = await fakeServers();
  try {
    const w = world(servers);
    const p = await w.alice.sdk.propose(TERMS, { acceptor: "bob@y.test" });
    await assert.rejects(w.bob.sdk.accept(p.id, { ...p.terms, price: { amount: "1.00", currency: "USD" } }), /commitment_mismatch/);
    const carol = new FakeAgent("carol@z.test");
    const csdk = new Agreements({ agent: carol, chain: w.chain, repo: REPO, verify: fakeVerify, obtainWarrants: carol.obtainAll });
    await assert.rejects(csdk.accept(p.id, p.terms), /not_acceptor/);
    // A proposer whose cap is too small is no longer caught at accept: the
    // proposal carries no credential to inspect (browserid-pay-3a8p), so the
    // acceptor cannot pre-flight it. The cap is enforced where the money moves
    // — the custodian refuses the LOCK — so the worst case is a wasted
    // acceptance, never work done for a payer who cannot pay.
    const p2 = await w.alice.sdk.propose(TERMS, { acceptor: "bob@y.test" });
    const acc = await w.bob.sdk.accept(p2.id, p2.terms);
    assert.ok(acc.acceptance, "accept succeeds without inspecting the proposer's grant");
    assert.equal(acc.acceptance.bundle, undefined, "no presentation is embedded in the acceptance");
  } finally { await servers.close(); }
});

test("public mode carries terms in the clear", async () => {
  const servers = await fakeServers();
  try {
    const w = world(servers);
    const p = await w.alice.sdk.propose(TERMS, { acceptor: "bob@y.test", mode: "public" });
    assert.equal(p.proposal.commitment, undefined);
    assert.equal(p.proposal.terms.price.amount, "50.00");
    await w.bob.sdk.accept(p.id); // no off-chain terms needed
  } finally { await servers.close(); }
});
