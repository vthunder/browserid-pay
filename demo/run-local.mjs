#!/usr/bin/env node
// Local end-to-end demo / test for browserid-pay.
//
// Starts: a stub daemon (in-process), demo-idp, the custodian (PAY_DEV) and
// the judge (JUDGE_DEV, canned ruling). Then two agents, alice and bob:
//   contract 1: propose → accept → fund → deliver → settle   (balances move)
//   contract 2: propose → accept → fund → deliver → dispute → judge rules
//               (split 30/20) → custodian settles per ruling
// Every step asserts state at the custodian and the chain; any failure exits
// non-zero. Run with `make demo-local` (cargo binaries are built first).
import { spawn } from "node:child_process";
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir, homedir } from "node:os";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import assert from "node:assert/strict";
import { Agreements, DaemonChain } from "@browserid-ng/agreements";
import { StubDaemon } from "./lib/daemon.mjs";
import { DemoAgent, verifyVia } from "./lib/agent.mjs";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
const TARGET = process.env.CARGO_TARGET_DIR || join(homedir(), ".cache", "cargo-target");
const BIN = (name) => join(TARGET, "debug", name);
const PORTS = { idp: 3900, pay: 3300, judge: 3400 };
const REPO = "sbo+raw://avail:turing:530/";
// Humans (accounts) and their delegated agents (the parties on chain).
const ALICE = "alice@x.test", BOB = "bob@y.test", JUDGE = "judge@dsp.test", ESCROW = "escrow@dsp.test", FUND = "fund@dsp.test";
const ALICE_AGENT = "alice+agent@x.test", BOB_AGENT = "bob+agent@y.test";

const children = [];
const tmp = mkdtempSync(join(tmpdir(), "browserid-pay-demo-"));
const log = (...a) => console.log("▸", ...a);
const step = (s) => console.log(`\n== ${s}`);

function run(name, bin, env) {
  const child = spawn(bin, [], { env: { ...process.env, ...env, RUST_LOG: process.env.RUST_LOG || "warn" }, stdio: ["ignore", "pipe", "pipe"] });
  child.stdout.on("data", (d) => process.env.DEMO_VERBOSE && process.stdout.write(`[${name}] ${d}`));
  child.stderr.on("data", (d) => process.env.DEMO_VERBOSE && process.stderr.write(`[${name}] ${d}`));
  child.on("exit", (code) => { if (code !== null && code !== 0 && !shuttingDown) console.error(`[${name}] exited with ${code}`); });
  children.push(child);
  return child;
}
let shuttingDown = false;
function shutdown() { shuttingDown = true; for (const c of children) c.kill("SIGTERM"); }

async function waitFor(what, fn, { tries = 100, ms = 100 } = {}) {
  for (let i = 0; i < tries; i++) {
    try { const v = await fn(); if (v) return v; } catch {}
    await new Promise((r) => setTimeout(r, ms));
  }
  throw new Error(`timeout waiting for ${what}`);
}
const getJson = (url) => fetch(url).then((r) => (r.ok ? r.json() : null));
const postJson = async (url, body, headers = {}) => {
  const r = await fetch(url, { method: "POST", headers: { "content-type": "application/json", ...headers }, body: JSON.stringify(body) });
  const j = await r.json().catch(() => ({}));
  if (!r.ok) throw new Error(`${url} → ${r.status}: ${JSON.stringify(j)}`);
  return j;
};

/** The stub daemon speaks the real read + submit routes; no override needed. */
const LocalChain = DaemonChain;

async function main() {
  // ---- infrastructure -------------------------------------------------------
  step("start the stub daemon, demo-idp, custodian, judge");
  const daemon = new StubDaemon();
  const daemonOrigin = await daemon.listen();
  const payOrigin = `http://127.0.0.1:${PORTS.pay}`;
  const judgeOrigin = `http://127.0.0.1:${PORTS.judge}`;
  daemon.put("/sys/", "contribution", { recipient: FUND, suggested_percentage: 1.0 }, "sys", "contribution.v1");
  daemon.put(`/providers/custodians/${ESCROW}/`, "listing", { v: 1, identity: ESCROW, origin: payOrigin, price: { amount: "0.50", currency: "USD" }, conformance: "browserid-pay-custodian-v1" }, ESCROW, "provider.v1");

  run("idp", BIN("demo-idp"), { DEMO_IDP_PORT: String(PORTS.idp) });
  const idp = await waitFor("demo-idp", () => getJson(`http://127.0.0.1:${PORTS.idp}/demo/issuer`));
  const trust = `${idp.domain}=${idp.public_key}`;
  log(`demo-idp ${idp.origin} (issuer ${idp.domain})`);

  run("pay", BIN("custodian"), {
    PAY_DEV: "1", PAY_PORT: String(PORTS.pay), PAY_ORIGIN: payOrigin, PAY_TRUST_ISSUER: trust, PAY_ACCEPTED_FALLBACKS: idp.domain,
    ADMIN_TOKEN: "demo", DATABASE_PATH: join(tmp, "custodian.db"), BROKER: idp.origin, PAY_SESSION_SECRET: "demo",
  });
  await waitFor("custodian", () => fetch(`${payOrigin}/healthz`).then((r) => r.ok));
  log(`custodian ${payOrigin}`);

  // The judge's identity: what a human approval at the broker would have produced.
  const jc = await postJson(`${idp.origin}/demo/credential`, { identity: JUDGE });
  const grantFor = (audience, scopes) => postJson(`${idp.origin}/demo/grant`, { grantor: JUDGE, grantee: JUDGE, audience, scopes, holder: jc.holder });
  const chainGrant = await grantFor(REPO, ["action:post", "path:/agreements/**", "path:/providers/adjudicators/**", `path:/${JUDGE}/**`]);
  const custGrant = await grantFor(payOrigin, ["ruling"]);
  const judgeDir = join(tmp, "judge");
  mkdirSync(judgeDir, { recursive: true });
  writeFileSync(join(judgeDir, "judge-credential.json"), JSON.stringify({
    credential: { device_key: jc.device_key, agent_device_cert: jc.agent_device_cert, idp: jc.idp, identity: JUDGE },
    grants: [{ audience: REPO, warrant: chainGrant.warrant, config_cert: chainGrant.config_cert }, { audience: payOrigin, warrant: custGrant.warrant, config_cert: custGrant.config_cert }],
  }));
  run("judge", BIN("judge"), {
    JUDGE_DEV: "1", JUDGE_PORT: String(PORTS.judge), JUDGE_ORIGIN: judgeOrigin, JUDGE_DATA_DIR: judgeDir, JUDGE_TRUST_ISSUER: trust,
    JUDGE_ACCEPTED_FALLBACKS: idp.domain, SBO_DAEMON: daemonOrigin, JUDGE_CHAIN_AUDIENCE: REPO, JUDGE_CUSTODIANS: payOrigin,
    JUDGE_SUBMIT_URL: `${daemonOrigin}/v1/submit`, BROKER: idp.origin, JUDGE_RESPONSE_WINDOW: "300",
    JUDGE_FAKE_RULING: JSON.stringify({ outcome: "split", split: { payee: "30.00", payer: "20.00" }, fee: "escrow", reasoning: "Half the summaries were delivered; the rest were not. Price split accordingly." }),
  });
  await waitFor("judge", () => fetch(`${judgeOrigin}/healthz`).then((r) => r.ok));
  const listing = await waitFor("judge provider listing on chain", () => daemon.get(`/providers/adjudicators/${JUDGE}/`, "listing"));
  assert.equal(listing.creator, JUDGE, "listing is attributed to the judge");
  assert.ok(listing.value.sealing_key, "listing carries the sealing key");
  log(`judge ${judgeOrigin} listed as ${JUDGE}, sealing key ${listing.value.sealing_key.slice(0, 12)}…`);

  // ---- agents --------------------------------------------------------------------
  step("two agents with test credit");
  const chain = new LocalChain({ daemon: daemonOrigin, repo: REPO });
  const mk = async (email) => {
    const agent = await DemoAgent.create(idp.origin, email);
    const sdk = new Agreements({ agent: agent.agent, chain, repo: REPO, verify: verifyVia(idp.origin), obtainWarrants: agent.obtainAll, broker: idp.origin });
    return { agent, sdk };
  };
  const alice = await mk(ALICE), bob = await mk(BOB);
  const credit = (identity, amount) => postJson(`${payOrigin}/admin/credit`, { identity, amount, memo: "demo" }, { authorization: "Bearer demo" });
  await credit(ALICE, "100.00");
  await credit(BOB, "10.00");
  const escrowOf = (id) => getJson(`${payOrigin}/api/escrow/${id}`);
  // Balances via the ledger session: sign in as the human (a self login presentation).
  const balance = async (who) => {
    const p = await who.agent.humanPresentationFor(payOrigin);
    const r = await fetch(`${payOrigin}/api/login`, { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ presentation: p }) });
    assert.ok(r.ok, `login as ${who.agent.human}: ${r.status}`);
    const cookie = r.headers.get("set-cookie").split(";")[0];
    const me = await fetch(`${payOrigin}/api/me`, { headers: { cookie } }).then((x) => x.json());
    return me;
  };
  assert.equal((await balance(alice)).balance, "100.00");
  assert.equal((await balance(bob)).balance, "10.00");
  log(`alice 100.00, bob 10.00`);

  const TERMS = {
    title: "Summarize 10 papers",
    description: "Bob's agent summarizes ten papers alice's agent names; delivery is a commitment over the summaries sent off-chain.",
    obligations: [{ by: BOB_AGENT, what: "deliver ten summaries", deadline: Math.floor(Date.now() / 1000) + 7 * 86400 }],
    price: { amount: "50.00", currency: "USD" },
    evidence: "a delivery commitment over the summaries",
  };

  // ---- contract 1: settle ---------------------------------------------------------
  step("contract 1 — propose, accept, fund, deliver, settle");
  const p1 = await alice.sdk.propose(TERMS, { acceptor: BOB_AGENT });
  log(`proposed ${p1.id}: total ${p1.terms.fees.custodian} + ${p1.terms.fees.adjudicator} fees, 1% contribution`);
  assert.equal(alice.agent.cards.length, 1, "one consent card for the whole agreement");
  const card = alice.agent.cards[0];
  assert.deepEqual(card.map((a) => a.audience), [`${REPO.replace(/\/+$/, "")}/agreements/${p1.id}`, REPO, judgeOrigin], "the card carries the contract, the standing ledger-write, and the judge's audience");
  assert.deepEqual(card[0].scopes[0].cap, { amount: "56.00", currency: "USD" });
  assert.deepEqual(card[0].scopes[0].counterparties, [BOB_AGENT]);
  assert.equal(daemon.get(`/agreements/${p1.id}/`, "proposal").creator, ALICE_AGENT, "authored by the agent, not the human");

  await bob.sdk.accept(p1.id, p1.terms);
  assert.equal(daemon.get(`/agreements/${p1.id}/`, "acceptance").creator, BOB_AGENT);
  assert.equal((await bob.sdk.status(p1.id)).stage, "binding");
  log("accepted (binding)");

  const lock = await alice.sdk.fund(p1.id);
  assert.equal(lock.lock.state, "locked");
  assert.equal(lock.lock.price, "50.00");
  assert.deepEqual([lock.lock.payer, lock.lock.payer_account, lock.lock.payee, lock.lock.payee_account], [ALICE_AGENT, ALICE, BOB_AGENT, BOB], "parties are the agents, accounts their humans");
  assert.equal((await balance(alice)).balance, "44.00", "100 − (50 + 0.50 + 5.00 + 0.50) locked");
  log("funded: alice 44.00 (56.00 in escrow)");

  const d1 = await bob.sdk.deliver(p1.id, { content: { summaries: ["…x10"] }, note: "all ten" });
  assert.match(d1.evidence.commitment, /^sha256:/);
  const s1 = await alice.sdk.settle(p1.id, { note: "great" });
  assert.equal(s1.release.state, "settled");
  assert.equal(s1.release.outcome, "acknowledged");
  const a1 = await balance(alice), b1 = await balance(bob);
  assert.equal(b1.balance, "60.00", "bob got the price");
  assert.equal(a1.balance, "49.00", "alice got the unused adjudicator fee back");
  assert.equal(a1.escrows[0].state, "settled");
  assert.equal((await alice.sdk.status(p1.id)).stage, "settled");
  log(`settled: bob 60.00, alice 49.00 (adjudicator fee returned); custodian fee 0.50, contribution 0.50 to ${FUND}`);

  // ---- contract 2: dispute --------------------------------------------------------
  step("contract 2 — propose, accept, fund, deliver, dispute, ruling");
  await credit(ALICE, "100.00");
  const p2 = await alice.sdk.propose(TERMS, { acceptor: BOB_AGENT });
  await bob.sdk.accept(p2.id, p2.terms);
  // Not enough for a second lock without the top-up: 149 − 56 = 93.
  await alice.sdk.fund(p2.id);
  assert.equal((await balance(alice)).balance, "93.00");
  await bob.sdk.deliver(p2.id, { content: { summaries: ["…x5"] }, note: "five of ten" });
  const cardsBefore = alice.agent.cards.length;
  const disp = await alice.sdk.dispute(p2.id, { note: "only half delivered" });
  assert.equal(alice.agent.cards.length, cardsBefore, "a dispute asks the human for nothing: the judge's audience was on the agreement's card");
  assert.equal(disp.frozen.escrow.state, "disputed");
  assert.ok(disp.frozen.sealed_terms, "custodian hands back the sealed terms");
  assert.equal(disp.filed.state, "open");
  assert.ok(disp.filed.respond_by > Date.now() / 1000 + 60, "the judge opened a response window");
  log(`disputed ${p2.id}: escrow frozen, case filed with the judge, bob may respond until ${disp.filed.respond_by}`);

  // Bob sees the dispute against him and responds by opening his delivery.
  const stB = await bob.sdk.status(p2.id);
  assert.equal(stB.stage, "disputed");
  assert.equal(stB.dispute.state, "open");
  assert.equal(stB.dispute.you_must_respond, true, "status tells bob he must respond");
  const caseBefore = await getJson(`${judgeOrigin}/api/cases/${p2.id}`);
  assert.equal(caseBefore.state, "open");
  assert.equal(caseBefore.ruling, null, "no ruling before bob's response");
  const resp = await bob.sdk.respond(p2.id, { note: "here are the five I delivered", reveal: true });
  assert.equal(resp.evidence.kind, "response");
  assert.equal(resp.evidence.reveals.length, 1, "one delivery opened");
  assert.equal(daemon.get(`/agreements/${p2.id}/evidence/`, resp.evidence_id).value.kind, "response", "response evidence on chain");
  assert.equal(resp.filed.reveals[0].matched, true, "the judge verified the reveal against the commitment");
  log(`bob responded: opened ${resp.evidence.reveals[0].evidence_id}, matched`);

  const kase = await waitFor("judge ruling delivered", async () => {
    const c = await getJson(`${judgeOrigin}/api/cases/${p2.id}`);
    if (c?.state === "failed") throw new Error(`judge failed: ${c.error}`);
    return c?.state === "delivered" ? c : null;
  }, { tries: 200, ms: 100 });
  assert.equal(kase.ruling.outcome, "split");
  const rulingObj = daemon.get(`/agreements/${p2.id}/`, "ruling");
  assert.equal(rulingObj?.creator, JUDGE, "ruling object on chain, attributed to the judge");
  assert.equal(daemon.get(`/${JUDGE}/attestations/${ALICE_AGENT}/`, "agreement:ruled")?.creator, JUDGE, "attestation on the agent that was a party");
  const e2 = await escrowOf(p2.id);
  assert.equal(e2.state, "settled");
  assert.equal(e2.outcome, "ruled:split");
  const a2 = await balance(alice), b2 = await balance(bob);
  assert.equal(b2.balance, "90.00", "bob: 60 + 30 (payee share)");
  assert.equal(a2.balance, "113.00", "alice: 93 + 20 (payer share); adjudicator fee spent");
  assert.equal((await alice.sdk.status(p2.id)).stage, "ruled");
  log(`ruled split 30/20: bob 90.00, alice 113.00, judge paid 5.00`);

  console.log("\nOK — two contracts: one settled by acknowledgement, one by ruling.");
}

main()
  .then(() => { shutdown(); rmSync(tmp, { recursive: true, force: true }); process.exit(0); })
  .catch((e) => { console.error("\nFAILED:", e); shutdown(); process.exit(1); });
