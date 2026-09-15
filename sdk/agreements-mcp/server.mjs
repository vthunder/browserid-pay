#!/usr/bin/env node
// MCP server over @browserid-ng/agreements. Identity: the same device
// credential file the browserid wallet keeps (AGREEMENTS_IDENTITY, default
// ~/.browserid-wallet/identity.json — see @browserid-ng/wallet). Warrants the
// human has not yet approved are raised at the broker; the approve URL is
// returned in the tool result so the host can show it.
//
// Env: AGREEMENTS_IDENTITY, BROWSERID_BROKER (https://browserid.me),
//      SBO_DAEMON (https://da.sandmill.org), SBO_REPO (sbo+raw://avail:turing:530/),
//      AGREEMENTS_TERMS_DIR (~/.browserid-agreements). Chain writes go through the daemon; no chain key.
import { readFileSync, writeFileSync, mkdirSync, existsSync } from "node:fs";
import { createHash } from "node:crypto";
import { homedir } from "node:os";
import { join } from "node:path";
import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import { z } from "zod";
import { DeviceAgent } from "@browserid-ng/agent";
import { Agreements, DaemonChain } from "@browserid-ng/agreements";

const IDENTITY = process.env.AGREEMENTS_IDENTITY || join(homedir(), ".browserid-wallet", "identity.json");
const TERMS_DIR = process.env.AGREEMENTS_TERMS_DIR || join(homedir(), ".browserid-agreements");
const BROKER = process.env.BROWSERID_BROKER || "https://browserid.me";
mkdirSync(TERMS_DIR, { recursive: true });

function loadAgent() {
  if (!existsSync(IDENTITY)) throw new Error(`no identity at ${IDENTITY} — provision one with @browserid-ng/wallet first`);
  const saved = JSON.parse(readFileSync(IDENTITY, "utf8"));
  const agent = new DeviceAgent(saved.credential || saved);
  for (const g of saved.grants || []) { try { agent.addGrant(g.grant); } catch {} }
  return { agent, saved };
}
function persistGrants(agent, saved) {
  saved.grants = agent.storedGrants();
  writeFileSync(IDENTITY, JSON.stringify(saved, null, 2));
}

// Terms live on disk per agreement (the chain never carries them in commitment mode).
const termsStore = {
  get: (id) => { const f = join(TERMS_DIR, `${id}.json`); return existsSync(f) ? JSON.parse(readFileSync(f, "utf8")) : undefined; },
  set: (id, v) => writeFileSync(join(TERMS_DIR, `${id}.json`), JSON.stringify(v, null, 2)),
};

let approveUrls = [];
let ctx;
function sdk() {
  if (ctx) return ctx;
  const { agent, saved } = loadAgent();
  const chain = new DaemonChain({ daemon: process.env.SBO_DAEMON, repo: process.env.SBO_REPO });
  const a = new Agreements({
    agent, chain, broker: BROKER, termsStore,
    onApproveUrl: (u) => approveUrls.push(u),
  });
  // Two-phase approvals: a warrant request needs the HUMAN to approve a link,
  // which can take minutes. Instead of blocking the tool call (the agent could
  // never show the link), start the request, return the link at once
  // (NeedsApproval), and finish on the next call of the same tool.
  const origObtain = a.obtainWarrant;
  a.obtainWarrant = async (aud, scopes) => {
    const key = aud + " " + JSON.stringify(scopes);
    let p = pending.get(key);
    if (!p) {
      const before = approveUrls.length;
      p = origObtain(aud, scopes).then(() => persistGrants(agent, saved)).finally(() => pending.delete(key));
      p.catch(() => {});
      pending.set(key, p);
      // Give the request a moment to file and surface its link.
      const deadline = Date.now() + 8000;
      while (approveUrls.length === before && Date.now() < deadline) {
        const done = await Promise.race([p.then(() => true, () => true), new Promise((r) => setTimeout(() => r(false), 250))]);
        if (done) return p;
      }
      if (approveUrls.length > before) throw new NeedsApproval(approveUrls.slice(before));
    }
    await p;
  };
  ctx = a;
  return a;
}

const pending = new Map();
const proposals = new Map(); // args-hash -> { id, salt } of a proposal awaiting approval
class NeedsApproval extends Error {
  constructor(urls) { super("needs approval"); this.urls = urls; }
}

const text = (s) => ({ content: [{ type: "text", text: s }] });
const run = async (fn) => {
  approveUrls = [];
  try {
    const out = await fn(sdk());
    const pre = approveUrls.length ? `APPROVE_URL: ${approveUrls.join("\n")}\n(the human approved these grants)\n\n` : "";
    return text(pre + JSON.stringify(out, null, 2));
  } catch (e) {
    if (e instanceof NeedsApproval) {
      return text(`APPROVAL NEEDED — show the human this link and wait for them to approve it, then call the same tool again with the same arguments:\n${e.urls.join("\n")}`);
    }
    const pre = approveUrls.length ? `APPROVE_URL: ${approveUrls.join("\n")}\n` : "";
    return text(`${pre}ERROR: ${e.message}`);
  }
};

const server = new McpServer({ name: "browserid-agreements", version: "0.1.0" });

const TERMS = {
  title: z.string().optional(),
  description: z.string().optional(),
  obligations: z.array(z.object({ by: z.string(), what: z.string(), deadline: z.number().optional() })),
  price: z.object({ amount: z.string(), currency: z.string() }),
  fees: z.object({ custodian: z.string().optional(), adjudicator: z.string().optional() }).optional(),
  evidence: z.string().optional(),
};

server.registerTool("propose", {
  title: "Propose an agreement",
  description: "Draft terms and post a proposal on the browserid-pay ledger. Returns the id and the FULL terms (with salt) that you must send to the counterparty off-chain — they need them to accept. Asks the human to approve a contract warrant (cap = price + fees + contribution) if none is held.",
  inputSchema: { ...TERMS, acceptor: z.string().describe("counterparty identity (email)"), payer: z.string().optional().describe("who pays; default: me"), expires: z.number().optional(), mode: z.enum(["commitment", "public"]).optional() },
}, (args) => run(async (a) => {
  // Same arguments ⇒ same proposal (id + salt), so a retry after the human's
  // approval resumes instead of minting a fresh id that no warrant matches.
  const { acceptor, payer, expires, mode, ...terms } = args;
  const key = createHash("sha256").update(JSON.stringify(args)).digest("hex");
  const resume = proposals.get(key) || {};
  try {
    const out = await a.propose(terms, { acceptor, payer, expires, mode, ...resume });
    proposals.delete(key);
    return out;
  } catch (e) {
    if (e instanceof NeedsApproval && e.resume) proposals.set(key, e.resume);
    throw e;
  }
}));

server.registerTool("accept", {
  title: "Accept a proposal",
  description: "Verify the proposer's authority and the terms' commitment, then post the acceptance. `terms` = the full terms the proposer sent you (omit for public proposals).",
  inputSchema: { id: z.string(), terms: z.record(z.any()).optional() },
}, ({ id, terms }) => run((a) => a.accept(id, terms)));

server.registerTool("fund", {
  title: "Fund escrow",
  description: "As the payer, lock price + fees + contribution at the custodian named in the proposal.",
  inputSchema: { id: z.string() },
}, ({ id }) => run((a) => a.fund(id)));

server.registerTool("deliver", {
  title: "Record a delivery",
  description: "Post a delivery commitment (hash of `content`) as evidence; send the content itself to the counterparty off-chain unless `public`.",
  inputSchema: { id: z.string(), content: z.any().optional(), commitment: z.string().optional(), note: z.string().optional(), public: z.boolean().optional() },
}, ({ id, ...o }) => run((a) => a.deliver(id, o)));

server.registerTool("settle", {
  title: "Acknowledge and release",
  description: "As the payer, acknowledge delivery on the ledger and release escrow to the payee.",
  inputSchema: { id: z.string(), note: z.string().optional() },
}, ({ id, note }) => run((a) => a.settle(id, { note })));

server.registerTool("dispute", {
  title: "Dispute",
  description: "Freeze escrow and file the dispute with the adjudicator named in the proposal (the sealed terms and evidence reach them).",
  inputSchema: { id: z.string(), note: z.string().optional() },
}, ({ id, note }) => run((a) => a.dispute(id, { note })));

server.registerTool("agreement_status", {
  title: "Agreement status",
  description: "Stage (proposed/binding/active/delivered/settled/disputed/ruled) with the on-chain objects and the custodian's escrow view.",
  inputSchema: { id: z.string() },
}, ({ id }) => run((a) => a.status(id)));

server.registerTool("discover", {
  title: "Discover providers",
  description: "List custodians, adjudicators and the suggested contribution on this ledger.",
  inputSchema: {},
}, () => run((a) => a.discover()));

await server.connect(new StdioServerTransport());
console.error(`browserid-agreements ready — identity ${IDENTITY}, broker ${BROKER}`);
