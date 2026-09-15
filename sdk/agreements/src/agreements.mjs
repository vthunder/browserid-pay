// The five verbs (plus fund and status). Wire shapes: docs/specs/agreement-v1.md.
import { randomBytes } from "node:crypto";
import { canonicalize, commitment, sha256hex } from "./jcs.mjs";
import { buildEnvelope, pubkeyHex } from "./envelope.mjs";
import { seal } from "./seal.mjs";
import { DaemonChain, DEFAULTS } from "./chain.mjs";

const AGREEMENT_SCOPE = "contract:agreement";
const CHAIN_SCOPES = (email) => ["action:post", "path:/agreements/**", `path:/${email}/**`];

export const contractAudience = (id, repo = DEFAULTS.repo) => `${repo.replace(/\/+$/, "")}/agreements/${id}`;
export const agreementPath = (id) => `/agreements/${id}/`;
const newId = () => "agr_" + randomBytes(12).toString("base64url");
const nowS = () => Math.floor(Date.now() / 1000);
const money = (s) => {
  const m = /^(\d+)(?:\.(\d{1,2}))?$/.exec(String(s ?? ""));
  if (!m) throw new Error(`bad amount: ${s}`);
  return Number(m[1]) * 100 + Number((m[2] || "0").padEnd(2, "0"));
};
const fmt = (minor) => `${Math.floor(minor / 100)}.${String(minor % 100).padStart(2, "0")}`;

// Provider listings live at `/providers/<kind>/<identity>/` with ID `listing`
// (agreement-v1.md "Providers"): the identity is a path segment because the
// root policy's grant patterns match paths, not ids. A prefix list of
// `/providers/<kind>/` returns every listing; keep only the `listing` objects
// whose identity matches their path segment (a listing under someone else's
// segment is unauthorized on chain, but a lenient reader could still see it).
function listings(objs) {
  return objs
    .filter((o) => o.id === "listing" && o.payload && typeof o.payload === "object")
    .filter((o) => o.path.endsWith(`/${o.payload.identity}/`))
    .map((o) => o.payload);
}
// The daemon's list route returns metadata only (no payload); fetch each
// listing object before filtering. A list entry that already carries a
// payload (the demo's stub daemon) is used as is.
async function listingsOf(chain, prefix) {
  const entries = (await chain.list(prefix)).filter((o) => o.id === "listing");
  const full = await Promise.all(entries.map(async (o) => (o.payload && typeof o.payload === "object") ? o : chain.get(o.path, o.id).catch(() => null)));
  return listings(full.filter(Boolean));
}

export class AgreementError extends Error {
  constructor(code, message) { super(message ? `${code}: ${message}` : code); this.code = code; }
}

/**
 * @param {object} o
 * @param {DeviceAgent} o.agent     from @browserid-ng/agent (email, deviceCert, addGrant, assertionFor, assertionWithAccessKey)
 * @param {object} [o.chain]        { get, list, submit } — default DaemonChain
 * @param {function} o.verify       async (presentation, audience) => { ok, email, grantee, scopes, reason }
 * @param {function} [o.obtainWarrants] async (grants: [{audience, scopes}]) => void — file ONE consent request for all of them (default: requestWarrants at the broker + onApproveUrl)
 * @param {function} [o.obtainWarrant] single-grant variant (mapped over the bundle when given instead)
 * @param {function} [o.onApproveUrl]
 * @param {string} [o.broker]
 * @param {object} [o.custodian]    { origin } override discovery
 * @param {object} [o.adjudicator]  { identity, origin, sealing_key, price } override discovery
 * @param {object} [o.contribution] { recipient, percentage } override /sys/contribution
 * @param {function} [o.http]
 */
export class Agreements {
  constructor(o) {
    if (!o?.agent) throw new Error("agent required");
    this.agent = o.agent;
    this.http = o.http || fetch;
    this.chain = o.chain || new DaemonChain({ http: this.http });
    this.repo = o.repo || this.chain.repo || DEFAULTS.repo;
    this.verify = o.verify || defaultVerify(this.http);
    this.broker = o.broker || "https://browserid.me";
    this.onApproveUrl = o.onApproveUrl || ((u) => console.error(`APPROVE: ${u}`));
    // One consent card per agreement: every grant an agreement needs is filed
    // as ONE request (`obtainWarrants(grants)`); `obtainWarrant` is the
    // single-grant shim. A caller-supplied single-grant hook is mapped.
    this.obtainWarrants = o.obtainWarrants
      || (o.obtainWarrant ? async (grants) => { for (const g of grants) await o.obtainWarrant(g.audience, g.scopes); } : (grants) => defaultObtainWarrants(this, grants));
    this.obtainWarrant = (aud, scopes) => this.obtainWarrants([{ audience: aud, scopes }]);
    this.overrides = { custodian: o.custodian, adjudicator: o.adjudicator, contribution: o.contribution };
    this.terms = o.termsStore || new Map(); // id -> { terms, salt }
  }

  get email() { return this.agent.email; }

  // ---- discovery -----------------------------------------------------------

  async discover() {
    const [cust, adj, contrib] = await Promise.all([
      this.overrides.custodian ? [this.overrides.custodian] : listingsOf(this.chain, "/providers/custodians/"),
      this.overrides.adjudicator ? [this.overrides.adjudicator] : listingsOf(this.chain, "/providers/adjudicators/"),
      this.overrides.contribution ? this.overrides.contribution : this.chain.get("/sys/", "contribution").then((o) => o?.payload || null),
    ]);
    return {
      custodians: cust, adjudicators: adj,
      contribution: contrib ? { recipient: contrib.recipient, percentage: contrib.suggested_percentage ?? contrib.percentage ?? 0 } : null,
    };
  }

  // ---- chain writes ----------------------------------------------------------

  async #ensureChainWarrant() {
    const aud = this.repo;
    if (!this.agent.warrantedAudiences().includes(aud)) await this.obtainWarrant(aud, CHAIN_SCOPES(this.email));
  }

  async post(path, id, payload, schema) {
    await this.#ensureChainWarrant();
    const { presentation, accessKey } = await this.agent.assertionWithAccessKey(this.repo);
    const body = Buffer.from(JSON.stringify(payload));
    const { wire } = buildEnvelope({
      action: "post", path, id, payload: body, schema,
      authCert: presentation, publicKeyHex: pubkeyHex(accessKey.publicRaw),
      sign: (bytes) => accessKey.signRaw(bytes),
    });
    const submission = await this.chain.submit(wire);
    return { submission, path, id };
  }

  // ---- contract warrants -----------------------------------------------------

  /**
   * Everything this agreement will ask the human for, in ONE card: the
   * per-contract grant, the standing ledger-write grant (first agreement
   * only), and the adjudicator's audience (so a later dispute needs no card).
   */
  async #contractWarrant(id, totalMinor, currency, counterparty, adjudicatorOrigin) {
    const aud = contractAudience(id, this.repo);
    const held = this.agent.warrantedAudiences();
    const grants = [];
    if (!held.includes(aud)) grants.push({ audience: aud, scopes: [{ scope: AGREEMENT_SCOPE, cap: { amount: fmt(totalMinor), currency }, counterparties: [counterparty] }] });
    if (!held.includes(this.repo)) grants.push({ audience: this.repo, scopes: CHAIN_SCOPES(this.email) });
    if (adjudicatorOrigin && !held.includes(adjudicatorOrigin)) grants.push({ audience: adjudicatorOrigin, scopes: ["dispute"] });
    if (grants.length) await this.obtainWarrants(grants);
    return this.agent.assertionFor(aud);
  }

  /** Verify a counterparty's bundle for the contract and check its cap covers the total. */
  async #checkBundle(id, bundle, expectIdentity, totalMinor, currency) {
    const r = await this.verify(bundle, contractAudience(id, this.repo));
    if (!r.ok) throw new AgreementError("bad_bundle", r.reason);
    if (r.grantee !== expectIdentity && r.email !== expectIdentity) throw new AgreementError("wrong_party", `${r.grantee} is not ${expectIdentity}`);
    const entry = (r.scopes || []).find((s) => (typeof s === "string" ? s : s?.scope) === AGREEMENT_SCOPE);
    if (!entry) throw new AgreementError("no_contract_scope");
    if (typeof entry === "object" && entry.cap) {
      if (entry.cap.currency !== currency || money(entry.cap.amount) < totalMinor) throw new AgreementError("cap_too_small", `cap ${entry.cap.amount} ${entry.cap.currency} < ${fmt(totalMinor)} ${currency}`);
      if (entry.counterparties && !entry.counterparties.some((c) => c === this.email || (c.startsWith("*@") && this.email.endsWith(c.slice(1))))) throw new AgreementError("counterparty_excluded");
    }
    return r;
  }

  // ---- propose ---------------------------------------------------------------

  /**
   * @param {object} terms   { title, description, obligations:[{by,what,deadline}], price:{amount,currency}, fees?:{custodian,adjudicator}, evidence }
   * @param {object} o       { acceptor, payer? (default: me), adjudicator?, custodian?, contribution?, expires?, mode? }
   * @returns {{ id, terms, salt, commitment, proposal, sealed_terms, submission }}
   */
  async propose(terms, o) {
    if (!o?.acceptor) throw new Error("acceptor required");
    // A retry (e.g. after a human approval) must resume the SAME proposal: the
    // contract warrant is bound to the id. Callers pass `o.id`/`o.salt` back;
    // any failure carries `e.resume = { id, salt }` for that purpose.
    const id = o.id || newId();
    const salt = o.salt || randomBytes(32).toString("base64url");
    try {
      return await this.#proposeWith(id, salt, terms, o);
    } catch (e) {
      if (e && typeof e === "object") e.resume = { id, salt };
      throw e;
    }
  }

  async #proposeWith(id, salt, terms, o) {
    const disc = await this.discover();
    const adjudicator = o.adjudicator || disc.adjudicators[0];
    const custodian = o.custodian || disc.custodians[0];
    if (!adjudicator?.identity) throw new AgreementError("no_adjudicator");
    if (!custodian?.origin) throw new AgreementError("no_custodian");
    const contribution = o.contribution || disc.contribution || { recipient: null, percentage: 0 };
    const payer = o.payer || this.email;
    const payee = payer === this.email ? o.acceptor : this.email;
    const fees = { custodian: terms.fees?.custodian ?? custodian.price?.amount ?? "0.00", adjudicator: terms.fees?.adjudicator ?? adjudicator.price?.amount ?? "0.00" };
    const full = { v: 1, id, salt, ...terms, fees, contribution: { recipient: contribution.recipient, percentage: contribution.percentage } };
    const com = commitment(full);
    const totalMinor = totalOf(full);
    const currency = full.price.currency;
    const bundle = await this.#contractWarrant(id, totalMinor, currency, o.acceptor, adjudicator.origin);
    const mode = o.mode || "commitment";
    const proposal = {
      v: 1, id, mode,
      ...(mode === "public" ? { terms: full } : { commitment: com }),
      parties: { proposer: this.email, acceptor: o.acceptor },
      roles: { payer, payee },
      adjudicator: adjudicator.identity, custodian: custodian.origin,
      contribution: { recipient: contribution.recipient, percentage: contribution.percentage },
      expires: o.expires || nowS() + 7 * 86400,
      bundle,
    };
    const { submission } = await this.post(agreementPath(id), "proposal", proposal, "agreement.proposal.v1");
    const sealed_terms = adjudicator.sealing_key ? seal(canonicalize(full), adjudicator.sealing_key) : null;
    this.terms.set(id, { terms: full, salt: full.salt, proposal, adjudicator, custodian });
    return { id, terms: full, salt: full.salt, commitment: com, proposal, sealed_terms, submission };
  }

  // ---- accept ----------------------------------------------------------------

  /** `terms` = the full terms the proposer sent off-chain (with salt). */
  async accept(id, terms) {
    const obj = await this.chain.get(agreementPath(id), "proposal");
    if (!obj) throw new AgreementError("no_proposal");
    const p = obj.payload;
    if (p.parties?.acceptor !== this.email) throw new AgreementError("not_acceptor", `proposal names ${p.parties?.acceptor}`);
    if (p.expires && p.expires < nowS()) throw new AgreementError("expired");
    const full = p.mode === "public" ? p.terms : terms;
    if (!full) throw new AgreementError("terms_required");
    if (p.mode !== "public" && commitment(full) !== p.commitment) throw new AgreementError("commitment_mismatch");
    if (full.id !== id) throw new AgreementError("terms_id_mismatch");
    const totalMinor = totalOf(full);
    const currency = full.price.currency;
    await this.#checkBundle(id, p.bundle, p.parties.proposer, totalMinor, currency);
    const adjudicator = await this.#adjudicatorFor(p);
    const bundle = await this.#contractWarrant(id, totalMinor, currency, p.parties.proposer, adjudicator?.origin);
    const proposal_hash = "sha256:" + sha256hex(Buffer.from(canonicalize(p)));
    const acceptance = { v: 1, id, proposal_hash, bundle };
    const { submission } = await this.post(agreementPath(id), "acceptance", acceptance, "agreement.acceptance.v1");
    this.terms.set(id, { terms: full, salt: full.salt, proposal: p, adjudicator, custodian: { origin: p.custodian } });
    return { id, acceptance, submission };
  }

  async #adjudicatorFor(p) {
    if (this.overrides.adjudicator?.identity === p.adjudicator) return this.overrides.adjudicator;
    const list = await listingsOf(this.chain, "/providers/adjudicators/");
    return list.find((a) => a?.identity === p.adjudicator) || { identity: p.adjudicator };
  }

  // ---- fund (payer) ------------------------------------------------------------

  async fund(id) {
    const st = this.terms.get(id);
    if (!st) throw new AgreementError("unknown_agreement", "propose/accept it first");
    const p = st.proposal;
    if (p.roles.payer !== this.email) throw new AgreementError("not_payer");
    const adjudicator = st.adjudicator?.sealing_key ? st.adjudicator : await this.#adjudicatorFor(p);
    const sealed_terms = adjudicator?.sealing_key ? seal(canonicalize(st.terms), adjudicator.sealing_key) : null;
    const presentation = await this.agent.assertionFor(contractAudience(id, this.repo));
    // The payee's bundle: its grantor is the account the price is released to.
    const payeeIsProposer = p.roles.payee === p.parties.proposer;
    const acc = payeeIsProposer ? null : await this.chain.get(agreementPath(id), "acceptance");
    const acceptance_bundle = payeeIsProposer ? p.bundle : acc?.payload?.bundle;
    if (!acceptance_bundle) throw new AgreementError("not_accepted", "no acceptance on chain yet");
    const body = {
      presentation, acceptance_bundle, proposal: p,
      price: st.terms.price, fees: st.terms.fees, contribution: p.contribution, sealed_terms,
    };
    return this.#custodian(p.custodian, "/api/escrow/lock", body);
  }

  // ---- deliver / settle / dispute ----------------------------------------------

  async #evidence(id, kind, extra = {}) {
    const ev = { v: 1, id, kind, by: this.email, ...extra };
    const eid = "ev_" + randomBytes(8).toString("base64url");
    const { submission } = await this.post(`${agreementPath(id)}evidence/`, eid, ev, "agreement.evidence.v1");
    return { evidence: ev, evidence_id: eid, submission };
  }

  /** Post a delivery: `{ content }` (hashed, or included when `public`) and/or `{ commitment }`, plus `note`. */
  async deliver(id, { content, commitment: com, note, public: pub } = {}) {
    const extra = { note };
    if (content !== undefined) {
      extra.commitment = "sha256:" + sha256hex(Buffer.from(typeof content === "string" ? content : canonicalize(content)));
      if (pub) extra.content = content;
    } else if (com) extra.commitment = com;
    const ev = await this.#evidence(id, "delivery", extra);
    if (content !== undefined) {
      // Keep the delivered content so a later `respond` can open the
      // commitment without the human re-pasting it.
      const st = this.terms.get(id) || {};
      st.deliveries = { ...(st.deliveries || {}), [ev.evidence_id]: content };
      this.terms.set(id, st);
    }
    return ev;
  }

  // ---- respond (the party a dispute is against) ------------------------------------

  /**
   * Answer a dispute within the judge's response window: post a `response`
   * evidence object (note + the hashes of what is being opened), then hand the
   * judge the opened content. `reveals`: `[{ evidence_id, content }]`, or
   * `{ reveal: true }` to open every delivery this agent made (kept locally by
   * `deliver`). The judge rules as soon as the response is in.
   */
  async respond(id, { note, reveals, reveal } = {}) {
    const p = await this.#proposal(id);
    const parties = [p.parties?.proposer, p.parties?.acceptor];
    if (!parties.includes(this.email)) throw new AgreementError("not_party");
    const st = this.terms.get(id) || {};
    let list = reveals || [];
    if (reveal) list = Object.entries(st.deliveries || {}).map(([evidence_id, content]) => ({ evidence_id, content }));
    const hashed = list.map((r) => ({ evidence_id: r.evidence_id, sha256: "sha256:" + sha256hex(Buffer.from(typeof r.content === "string" ? r.content : canonicalize(r.content))) }));
    const ev = await this.#evidence(id, "response", { note, reveals: hashed });
    const adjudicator = await this.#adjudicatorFor(p);
    if (!adjudicator?.origin) throw new AgreementError("no_adjudicator", "the proposal names no reachable adjudicator");
    if (!this.agent.warrantedAudiences().includes(adjudicator.origin)) await this.obtainWarrant(adjudicator.origin, ["dispute"]);
    const jp = await this.agent.assertionFor(adjudicator.origin);
    const filed = await this.#json(`${adjudicator.origin.replace(/\/+$/, "")}/api/cases/${id}/respond`, { presentation: jp, note, reveals: list });
    return { ...ev, filed };
  }

  // ---- evidence patterns (docs/specs/evidence-patterns.md) -----------------------

  /**
   * Input handoff: the party providing inputs commits to them on the ledger
   * (`kind: "handoff"`) and sends them off-chain; the receiver answers with
   * `acknowledgeReceipt`. `obligation` = index of the input obligation this
   * fulfils (optional when the terms have one input obligation).
   */
  async handoff(id, { content, commitment: com, manifest, obligation, note } = {}) {
    const extra = { note };
    if (content !== undefined) {
      extra.commitment = contentCommitment(content);
      const st = this.terms.get(id) || {};
      this.terms.set(id, { ...st, handoffs: { ...(st.handoffs || {}), [extra.commitment]: content } });
      if (!manifest) manifest = defaultManifest(content);
    } else if (com) extra.commitment = com;
    else throw new AgreementError("bad_request", "handoff needs content or a commitment");
    if (manifest) extra.manifest = manifest;
    if (obligation !== undefined) extra.obligation = obligation;
    return this.#evidence(id, "handoff", extra);
  }

  /**
   * Receipt for a handoff: `content` (what was received) is hashed and checked
   * against the handoff's commitment — a mismatch is posted as contested with
   * the reason; `contested` posts a contest without content.
   */
  async acknowledgeReceipt(id, { handoff_id, content, contested } = {}) {
    if (!handoff_id) throw new AgreementError("bad_request", "handoff_id required");
    const h = await this.chain.get(`${agreementPath(id)}evidence/`, handoff_id);
    if (!h || h.payload?.kind !== "handoff") throw new AgreementError("no_handoff", handoff_id);
    let ok, reason;
    if (contested) { ok = false; reason = String(contested); }
    else if (content !== undefined) {
      const got = contentCommitment(content);
      ok = got === h.payload.commitment;
      if (!ok) reason = `received content hashes to ${got}, handoff committed to ${h.payload.commitment}`;
    } else throw new AgreementError("bad_request", "receipt needs the received content, or contested");
    return this.#evidence(id, "receipt", { handoff: handoff_id, ok, ...(reason ? { reason } : {}), obligation: h.payload.obligation });
  }

  /**
   * Each obligation with its state, from the terms (local or public), the
   * evidence on chain and the clock: met | pending | late | blocked (its
   * dependency is unmet). Deadlines slide by a dependency's lateness. The
   * counterparty of a late obligation holds the dispute right.
   */
  async obligations(id) {
    const st = this.terms.get(id);
    let terms = st?.terms;
    const p = await this.#proposal(id);
    if (!terms && p?.terms) terms = p.terms;
    if (!terms) throw new AgreementError("no_terms", "the private terms are not held locally and the proposal is not public");
    const path = agreementPath(id);
    const [lock, evidence] = await Promise.all([this.chain.get(path, "lock"), this.chain.list(`${path}evidence/`)]);
    // The payload's own `id` is the AGREEMENT id; the evidence id is the object id.
    const evs = evidence.map((e) => ({ ...e.payload, agreement: e.payload?.id, id: e.id, block: e.block ?? null }));
    return deriveObligations(terms.obligations || [], { evidence: evs, lock: !!lock, now: nowS(), parties: p.parties, roles: p.roles });
  }

  /** Payer acknowledges delivery: evidence on chain, then the custodian releases. */
  async settle(id, { note } = {}) {
    const p = await this.#proposal(id);
    if (p.roles.payer !== this.email) throw new AgreementError("not_payer");
    const ev = await this.#evidence(id, "acknowledgement", { note });
    const presentation = await this.agent.assertionFor(contractAudience(id, this.repo));
    const release = await this.#custodian(p.custodian, `/api/escrow/${id}/acknowledge`, { presentation });
    return { ...ev, release };
  }

  /** Either party disputes: evidence on chain, freeze at the custodian, file with the adjudicator. */
  async dispute(id, { note } = {}) {
    // Safe to retry: the human-facing step (the judge's sign-in card) comes
    // BEFORE any side effect; a dispute note already on chain by me is reused;
    // an escrow the custodian already froze counts as frozen.
    const p = await this.#proposal(id);
    const adjudicator = await this.#adjudicatorFor(p);
    if (adjudicator?.origin && !this.agent.warrantedAudiences().includes(adjudicator.origin)) {
      await this.obtainWarrant(adjudicator.origin, ["dispute"]);
    }
    const existing = (await this.chain.list(`${agreementPath(id)}evidence/`))
      .find((o) => o.payload?.kind === "dispute" && o.payload?.by === this.email);
    const ev = existing
      ? { evidence: existing.payload, evidence_id: existing.id, submission: null, reused: true }
      : await this.#evidence(id, "dispute", { note });
    const presentation = await this.agent.assertionFor(contractAudience(id, this.repo));
    let frozen;
    try {
      frozen = await this.#custodian(p.custodian, `/api/escrow/${id}/dispute`, { presentation });
    } catch (e) {
      if (e.code !== "bad_state") throw e;
      frozen = await this.#custodian(p.custodian, `/api/escrow/${id}`, null); // already disputed: read the state
      if (frozen?.state !== "disputed") throw e;
    }
    let filed = null;
    if (adjudicator?.origin) {
      const st = this.terms.get(id);
      const sealed_terms = frozen?.sealed_terms || (st && adjudicator.sealing_key ? seal(canonicalize(st.terms), adjudicator.sealing_key) : null);
      const jp = await this.agent.assertionFor(adjudicator.origin);
      try {
        filed = await this.#json(`${adjudicator.origin.replace(/\/+$/, "")}/api/dispute`, { presentation: jp, contract_id: id, sealed_terms, note });
      } catch (e) {
        if (e.code !== "http_409") throw e; // already filed
        filed = { already_filed: true };
      }
    }
    return { ...ev, frozen, filed };
  }


  /** Payer acknowledges delivery: evidence on chain, then the custodian releases. */
  async settle(id, { note } = {}) {
    const p = await this.#proposal(id);
    if (p.roles.payer !== this.email) throw new AgreementError("not_payer");
    const ev = await this.#evidence(id, "acknowledgement", { note });
    const presentation = await this.agent.assertionFor(contractAudience(id, this.repo));
    const release = await this.#custodian(p.custodian, `/api/escrow/${id}/acknowledge`, { presentation });
    return { ...ev, release };
  }

  /** Either party disputes: evidence on chain, freeze at the custodian, file with the adjudicator. */
  async dispute(id, { note } = {}) {
    // Safe to retry: the human-facing step (the judge's sign-in card) comes
    // BEFORE any side effect; a dispute note already on chain by me is reused;
    // an escrow the custodian already froze counts as frozen.
    const p = await this.#proposal(id);
    const adjudicator = await this.#adjudicatorFor(p);
    if (adjudicator?.origin && !this.agent.warrantedAudiences().includes(adjudicator.origin)) {
      await this.obtainWarrant(adjudicator.origin, ["dispute"]);
    }
    const existing = (await this.chain.list(`${agreementPath(id)}evidence/`))
      .find((o) => o.payload?.kind === "dispute" && o.payload?.by === this.email);
    const ev = existing
      ? { evidence: existing.payload, evidence_id: existing.id, submission: null, reused: true }
      : await this.#evidence(id, "dispute", { note });
    const presentation = await this.agent.assertionFor(contractAudience(id, this.repo));
    let frozen;
    try {
      frozen = await this.#custodian(p.custodian, `/api/escrow/${id}/dispute`, { presentation });
    } catch (e) {
      if (e.code !== "bad_state") throw e;
      frozen = await this.#custodian(p.custodian, `/api/escrow/${id}`, null); // already disputed: read the state
      if (frozen?.state !== "disputed") throw e;
    }
    let filed = null;
    if (adjudicator?.origin) {
      const st = this.terms.get(id);
      const sealed_terms = frozen?.sealed_terms || (st && adjudicator.sealing_key ? seal(canonicalize(st.terms), adjudicator.sealing_key) : null);
      const jp = await this.agent.assertionFor(adjudicator.origin);
      try {
        filed = await this.#json(`${adjudicator.origin.replace(/\/+$/, "")}/api/dispute`, { presentation: jp, contract_id: id, sealed_terms, note });
      } catch (e) {
        if (e.code !== "http_409") throw e; // already filed
        filed = { already_filed: true };
      }
    }
    return { ...ev, frozen, filed };
  }

  // ---- status ------------------------------------------------------------------

  async status(id) {
    const path = agreementPath(id);
    const [proposal, acceptance, lock, ruling, evidence] = await Promise.all([
      this.chain.get(path, "proposal"), this.chain.get(path, "acceptance"), this.chain.get(path, "lock"),
      this.chain.get(path, "ruling"), this.chain.list(`${path}evidence/`),
    ]);
    let stage = "none";
    if (proposal) stage = "proposed";
    if (proposal && acceptance && (acceptance.block == null || proposal.block == null || acceptance.block >= proposal.block)) stage = "binding";
    if (stage === "binding" && lock) stage = "active";
    const kinds = evidence.map((e) => e.payload?.kind);
    if (stage === "active" && kinds.includes("delivery")) stage = "delivered";
    if (stage !== "none" && kinds.includes("acknowledgement")) stage = "settled";
    if (kinds.includes("dispute")) stage = "disputed";
    if (ruling) stage = "ruled";
    // While disputed, the judge's case shows the response window and whether
    // the other side has answered.
    let dispute = null;
    if (stage === "disputed" && proposal?.payload) {
      try {
        const adj = await this.#adjudicatorFor(proposal.payload);
        if (adj?.origin) {
          const c = await this.http(`${adj.origin.replace(/\/+$/, "")}/api/cases/${id}`).then((r) => (r.ok ? r.json() : null)).catch(() => null);
          if (c) {
            const disputedAgainst = (proposal.payload.parties ? [proposal.payload.parties.proposer, proposal.payload.parties.acceptor] : []).find((x) => x !== c.disputed_by) || null;
            dispute = { state: c.state, disputed_by: c.disputed_by, against: disputedAgainst, respond_by: c.respond_by, responded: !!c.response, you_must_respond: disputedAgainst === this.email && c.state === "open" };
          }
        }
      } catch {}
    }
    let escrow = null;
    if (proposal?.payload?.custodian) {
      escrow = await this.http(`${proposal.payload.custodian.replace(/\/+$/, "")}/api/escrow/${id}`).then((r) => (r.ok ? r.json() : null)).catch(() => null);
    }
    return { id, stage, proposal: proposal?.payload || null, acceptance: acceptance?.payload || null, lock: lock?.payload || null, ruling: ruling?.payload || null, evidence: evidence.map((e) => ({ id: e.id, ...e.payload })), escrow, dispute };
  }

  // ---- helpers -------------------------------------------------------------------

  async #proposal(id) {
    const st = this.terms.get(id);
    if (st?.proposal) return st.proposal;
    const obj = await this.chain.get(agreementPath(id), "proposal");
    if (!obj) throw new AgreementError("no_proposal");
    return obj.payload;
  }

  #custodian(origin, route, body) { return this.#json(`${origin.replace(/\/+$/, "")}${route}`, body); }

  async #json(url, body) {
    const r = body === null
      ? await this.http(url, { method: "GET" })
      : await this.http(url, { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(body) });
    const j = await r.json().catch(() => ({}));
    if (!r.ok) throw new AgreementError(j.error || `http_${r.status}`, j.detail || j.reason || `${url} → ${r.status}`);
    return j;
  }
}

/** price + fees + contribution, in minor units. */
export function totalOf(terms) {
  const price = money(terms.price.amount);
  const fees = money(terms.fees?.custodian ?? "0") + money(terms.fees?.adjudicator ?? "0");
  const pct = Number(terms.contribution?.percentage ?? 0);
  const contrib = Math.ceil((price * pct) / 100);
  return price + fees + contrib;
}

function defaultVerify(http) {
  let verifier;
  return async (presentation, audience) => {
    if (!verifier) {
      const { createVerifier } = await import("@browserid-ng/verify");
      verifier = createVerifier({ fetch: http });
    }
    return verifier.verify(presentation, audience);
  };
}

async function defaultObtainWarrants(self, grants) {
  const { requestWarrants } = await import("@browserid-ng/agent");
  const pending = await requestWarrants(self.broker, {
    deviceCert: self.agent.deviceCert, identity: self.email,
    grants: grants.map((g) => ({ audience: g.audience, scopes: g.scopes })),
    message: grants.some((g) => g.audience.includes("/agreements/"))
      ? "Enter an agreement on your behalf (and record it on the browserid-pay ledger)."
      : "Record agreements and attestations on the browserid-pay ledger.",
    http: self.http,
  });
  self.onApproveUrl(pending.verificationUriComplete || pending.verificationUri);
  const granted = await pending.wait();
  for (const g of granted) self.agent.addGrant(g.grant);
  const held = self.agent.warrantedAudiences();
  for (const g of grants) if (!held.includes(g.audience)) throw new AgreementError("warrant_not_granted", g.audience);
}
