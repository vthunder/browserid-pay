// The five verbs (plus fund and status). Wire shapes: docs/specs/agreement-v1.md.
import { randomBytes } from "node:crypto";
import { canonicalize, commitment, sha256hex } from "./jcs.mjs";
import { buildEnvelope, pubkeyHex } from "./envelope.mjs";
import { seal } from "./seal.mjs";
import { DaemonChain, DEFAULTS } from "./chain.mjs";

const AGREEMENT_SCOPE = "contract:agreement";
const CHAIN_SCOPES = (email) => ["action:post", "path:/agreements/**", `path:/${email}/**`];

export const contractAudience = (id, repo = DEFAULTS.repo) => `${collectionAudience(repo)}/${id}`;
/**
 * The audience a STANDING agreement warrant names: the agreements collection on
 * the ledger, not one agreement and not one custodian. A grant here covers every
 * agreement its parameters admit, so the human approves the shape of the deals
 * once instead of each deal. Per-agreement warrants remain valid and are
 * preferred when held — a blanket grant for small agreements plus a one-off
 * approval for a large one is the intended combination.
 */
/**
 * How long the recipient has to respond to a delivery before silence is taken
 * as acceptance. A ledger default; an agreement may set its own in the terms.
 * Short enough that withholding cannot stall, long enough for a human to look.
 */
export const DEFAULT_RESPONSE_WINDOW = "P2D";
// Fixed by the protocol for now rather than negotiated per agreement. Making
// them terms invites a proposer to set a window the counterparty cannot meet;
// negotiable timings need enforced floors first (browserid-pay-b3gq).
/**
 * The absolute limit on settling whether a delivery ARRIVED, measured from the
 * FIRST delivery and never restarted by a cure. Without it, "I never got it"
 * resets the clock every time and stalls indefinitely.
 *
 * Default: three response windows. It only bites while an arrival is actively
 * disputed — silence is still acceptance, and a quality complaint (`not_met`)
 * acknowledges receipt and leaves this clock behind.
 */
export const DEFAULT_RESOLUTION_MULTIPLE = 3;
/** ISO-8601 duration → seconds. Days/hours/minutes/seconds only. */
export function windowSecs(iso) {
  const m = /^P(?:(\d+)D)?(?:T(?:(\d+)H)?(?:(\d+)M)?(?:(\d+)S)?)?$/.exec(String(iso || ""));
  if (!m || !m.slice(1).some(Boolean)) return null;
  const [d, h, mi, sec] = m.slice(1).map((x) => Number(x || 0));
  return d * 86400 + h * 3600 + mi * 60 + sec;
}

export const collectionAudience = (repo = DEFAULTS.repo) => `${repo.replace(/\/+$/, "")}/agreements`;
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
 * @param {object} [o.contribution] { recipient, fn?, amount?, percentage } override /sys/contribution
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
    // The indexer whose scorecards reputation-gated grants default to.
    this.index = (o.index || DEFAULTS.index || "https://index.dsp.fyi").replace(/\/+$/, "");
    this.onApproveUrl = o.onApproveUrl || ((u) => console.error(`APPROVE: ${u}`));
    // Extra grants a host wants on the SAME agreement card (e.g. the relay's
    // `message` scope), so a run is one card per agent per agreement.
    this.extraGrants = o.extraGrants || [];
    // One consent card per agreement: every grant an agreement needs is filed
    // as ONE request (`obtainWarrants(grants)`); `obtainWarrant` is the
    // single-grant shim. A caller-supplied single-grant hook is mapped.
    this.obtainWarrants = o.obtainWarrants
      || (o.obtainWarrant ? async (grants) => { for (const g of grants) await o.obtainWarrant(g.audience, g.scopes); } : (grants) => defaultObtainWarrants(this, grants));
    this.obtainWarrant = (aud, scopes) => this.obtainWarrants([{ audience: aud, scopes }]);
    this.overrides = { custodian: o.custodian, adjudicator: o.adjudicator, contribution: o.contribution };
    this.terms = o.termsStore || new Map(); // id -> { terms, salt }
    // What this agent is TRYING to do, independent of any agreement. Local and
    // persisted by the host (never sent anywhere): intent is the human's, and
    // an agent that forgets it between ticks cannot run a loop.
    this.goals = o.goalStore || new Map(); // goal id -> record
    // Timing is fixed by the protocol, not negotiated — a window the proposer
    // picks is a window an attacker tunes. These override the defaults LOCALLY
    // (tests, and a deployment that wants a different tempo); they are not
    // published, so a counterparty cannot set yours. Both sides should use the
    // same values or their views of whose turn it is will drift.
    this.windows = { response: DEFAULT_RESPONSE_WINDOW, resolution: null, ...(o.windows || {}) };
  }

  get email() { return this.agent.email; }

  // ---- discovery -----------------------------------------------------------

  async discover() {
    const [cust, adj, contrib] = await Promise.all([
      this.overrides.custodian ? [this.overrides.custodian] : listingsOf(this.chain, "/providers/custodians/"),
      this.overrides.adjudicator ? [this.overrides.adjudicator] : listingsOf(this.chain, "/providers/adjudicators/"),
      this.overrides.contribution ? this.overrides.contribution : this.chain.get("/sys/", "contribution").then((o) => o?.payload || null),
    ]);
    const me = await this.reputationOf(this.email).catch(() => null);
    return {
      custodians: cust, adjudicators: adj,
      contribution: contrib
        ? normalizeContribution({
            recipient: contrib.recipient,
            fn: contrib.fn,
            amount: contrib.suggested_amount ?? contrib.amount,
            percentage: contrib.suggested_percentage ?? contrib.percentage ?? 0,
          })
        : null,
      index: this.index,
      my_reputation: me ? { score: me.score, n_counterparty_accounts: me.n_counterparty_accounts, formula: me.formula, indexer: this.index } : null,
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
  async #contractWarrant(id, totalMinor, currency, counterparty, adjudicatorOrigin, minReputation) {
    const aud = contractAudience(id, this.repo);
    const standing = collectionAudience(this.repo);
    const held = this.agent.warrantedAudiences();
    const grants = [];
    // Prefer authority already held, narrowest first: a one-off approval for
    // THIS agreement beats a blanket one, and either beats asking again. The
    // custodian re-checks the grant's cap and counterparties at lock, so
    // reaching for the standing warrant here is a convenience, not a bypass —
    // if it does not actually cover this deal the lock refuses and the agent
    // can ask for a one-off.
    if (!held.includes(aud) && held.includes(standing)) {
      if (adjudicatorOrigin && !held.includes(adjudicatorOrigin)) grants.push({ audience: adjudicatorOrigin, scopes: ["dispute"] });
      if (!held.includes(this.repo)) grants.push({ audience: this.repo, scopes: CHAIN_SCOPES(this.email) });
      for (const g of this.extraGrants) if (!held.includes(g.audience)) grants.push(g);
      if (grants.length) await this.obtainWarrants(grants);
      return this.agent.assertionFor(standing);
    }
    if (!held.includes(aud)) {
      // A reputation-gated grant names a threshold at an indexer instead of
      // (or as well as) the counterparty: "any agent rated ≥ N at …".
      const entry = { scope: AGREEMENT_SCOPE, cap: { amount: fmt(totalMinor), currency } };
      if (minReputation) entry.min_reputation = { indexer: (minReputation.indexer || this.index).replace(/\/+$/, ""), score: Math.round(Number(minReputation.score)) };
      if (!minReputation || minReputation.also_counterparty) entry.counterparties = [counterparty];
      grants.push({ audience: aud, scopes: [entry] });
    }
    if (!held.includes(this.repo)) grants.push({ audience: this.repo, scopes: CHAIN_SCOPES(this.email) });
    if (adjudicatorOrigin && !held.includes(adjudicatorOrigin)) grants.push({ audience: adjudicatorOrigin, scopes: ["dispute"] });
    for (const g of this.extraGrants) if (!held.includes(g.audience)) grants.push(g);
    if (grants.length) await this.obtainWarrants(grants);
    return this.agent.assertionFor(aud);
  }

  /**
   * Ask the human once for a STANDING agreement grant, so later agreements need
   * no card at all. `cap` is the ceiling across every agreement in `window`
   * (an ISO-8601 duration, rolling from the warrant's issue time), not per
   * agreement. `counterparties` may name identities, `"*@domain"`, or `"*"`.
   *
   * Deliberately explicit: nothing requests this implicitly, because the card
   * it produces is much broader than a single agreement's.
   */
  async requestStandingConsent({ cap, window, counterparties, min_reputation, max_duration } = {}) {
    if (!cap?.amount || !cap?.currency) throw new AgreementError("bad_request", "a standing grant needs cap { amount, currency }");
    const aud = collectionAudience(this.repo);
    if (this.agent.warrantedAudiences().includes(aud)) {
      return { audience: aud, already_held: true };
    }
    const entry = { scope: AGREEMENT_SCOPE, cap: { amount: cap.amount, currency: cap.currency, ...(window ? { window } : {}) } };
    if (counterparties?.length) entry.counterparties = counterparties;
    if (max_duration) entry.max_duration = max_duration;
    if (min_reputation) entry.min_reputation = { indexer: (min_reputation.indexer || this.index).replace(/\/+$/, ""), score: Math.round(Number(min_reputation.score)) };
    const grants = [{ audience: aud, scopes: [entry] }];
    const held = this.agent.warrantedAudiences();
    if (!held.includes(this.repo)) grants.push({ audience: this.repo, scopes: CHAIN_SCOPES(this.email) });
    // Bundle the default adjudicator's `dispute` grant into the SAME card.
    // Otherwise the first agreement under a "no more cards" standing grant
    // still produces one, which is exactly the promise this is meant to keep.
    // An agreement naming a DIFFERENT adjudicator will still ask — correctly,
    // since that is authority the human has not given.
    try {
      const disc = await this.discover();
      const adj = this.overrides.adjudicator || disc.adjudicators?.[0];
      if (adj?.origin && !held.includes(adj.origin)) grants.push({ audience: adj.origin, scopes: ["dispute"] });
    } catch { /* discovery is a convenience here, not a requirement */ }
    await this.obtainWarrants(grants);
    return { audience: aud, granted: entry, bundled: grants.map((g) => g.audience) };
  }

  /**
   * Tell the custodian which account this identity's money belongs to, by
   * presenting a contract grant once. The custodian keeps the mapping, so at
   * lock time it can pay a payee who is offline and presents nothing.
   */
  async #registerWithCustodian(origin, id) {
    if (!origin) return;
    const presentation = await this.#contractAssertion(id);
    const held = this.agent.warrantedAudiences();
    const agreement = held.includes(contractAudience(id, this.repo)) ? id : undefined;
    return this.#custodian(origin, "/api/escrow/register", { presentation, agreement });
  }

  /**
   * My presentation for an agreement, under whichever authority I actually
   * hold: a warrant for THIS agreement if there is one, else the standing
   * warrant on the collection. Minting for the wrong audience is an
   * "assertion: audience mismatch" the counterparty cannot diagnose.
   */
  async #contractAssertion(id) {
    const aud = contractAudience(id, this.repo);
    const held = this.agent.warrantedAudiences();
    if (!held.includes(aud) && held.includes(collectionAudience(this.repo))) {
      return this.agent.assertionFor(collectionAudience(this.repo));
    }
    return this.agent.assertionFor(aud);
  }

  /**
   * Nominate a replacement adjudicator, when the named one has not ruled.
   *
   * Posts the nomination on chain and then asks the custodian to swap. The swap
   * only happens once BOTH parties have nominated the SAME identity and the
   * original's ruling window has lapsed — so the first caller posts and is told
   * `not_agreed`, which is the normal, expected outcome until the other side
   * agrees. The escrowed fee follows whoever rules.
   */
  async replaceAdjudicator(id, adjudicator) {
    if (!adjudicator) throw new AgreementError("bad_request", "name the replacement adjudicator");
    const p = await this.#proposal(id);
    const ev = await this.#evidence(id, "replace_adjudicator", { adjudicator });
    const presentation = await this.#contractAssertion(id);
    const swap = await this.#custodian(p.custodian, `/api/escrow/${id}/adjudicator`, { presentation })
      .catch((e) => ({ pending: e.code || "not_agreed", detail: e.message }));
    return { ...ev, swap };
  }

  /** One indexer's signed scorecard for `subject` (default: the configured index). */
  async reputationOf(subject, indexer = this.index) {
    const r = await this.http(`${indexer.replace(/\/+$/, "")}/v1/reputation?subject=${encodeURIComponent(subject)}`);
    if (!r.ok) throw new AgreementError("reputation_unavailable", `${indexer} → ${r.status}`);
    return r.json();
  }

  /** Verify a counterparty's bundle for the contract and check its cap covers the total. */
  /**
   * My presentation for an agreement, under whichever authority I actually
   * hold: a warrant for THIS agreement if there is one, else the standing
   * warrant on the collection. Minting for the wrong audience is an
   * "assertion: audience mismatch" the counterparty cannot diagnose.
   */

  // ---- propose ---------------------------------------------------------------

  /**
   * @param {object} terms   { title, description, obligations:[{by,what,deadline}], price:{amount,currency}, fees?:{custodian,adjudicator}, evidence }
   * @param {object} o       { acceptor, payer? (default: me), adjudicator?, custodian?, contribution?, expires?, mode?, min_reputation?: { indexer?, score, also_counterparty? }, goal? (goal id to attach) }
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
    const contributionOut = normalizeContribution(contribution);
    const full = { v: 1, id, salt, ...terms, fees, contribution: contributionOut };
    const com = commitment(full);
    const totalMinor = totalOf(full);
    const currency = full.price.currency;
    // Obtain the warrants (this is the human's consent card) but do NOT embed
    // the presentation: a permanent object cannot carry a five-minute
    // assertion, and it does not need to — the chain attributes this write to
    // us, which is the same proof (browserid-pay-3a8p).
    await this.#contractWarrant(id, totalMinor, currency, o.acceptor, adjudicator.origin, o.min_reputation);
    await this.#registerWithCustodian(custodian.origin, id).catch(() => {});
    const mode = o.mode || "commitment";
    const proposal = {
      v: 1, id, mode,
      ...(mode === "public" ? { terms: full } : { commitment: com }),
      parties: { proposer: this.email, acceptor: o.acceptor },
      roles: { payer, payee },
      adjudicator: adjudicator.identity, custodian: custodian.origin,
      // Declared in the clear so the acceptor can check itself against it
      // before doing any work. The AUTHORITY is still the proposer's grant,
      // which the custodian reads at lock — this is the courtesy copy, and a
      // proposal that understates it simply fails there.
      ...(o.min_reputation ? { min_reputation: { indexer: (o.min_reputation.indexer || this.index).replace(/\/+$/, ""), score: Math.round(Number(o.min_reputation.score)) } } : {}),
      contribution: contributionOut,
      // Fees are CLEAR on the proposal even in commitment mode: a chain policy
      // can only cap what it can read, and capping adjudication / escrow fees is
      // an ecosystem-health lever. This publishes fee pricing, never deal value.
      fees,
      expires: o.expires || nowS() + 7 * 86400,
    };
    const { submission } = await this.post(agreementPath(id), "proposal", proposal, "agreement.proposal.v1");
    // Attach the goal that motivated this, so the pipeline advances instead of
    // proposing the same thing again on the next tick.
    if (o.goal && this.goals.get(o.goal)) this.updateGoal(o.goal, { agreement: id, state: "running" });
    const sealed_terms = adjudicator.sealing_key ? seal(canonicalize(full), adjudicator.sealing_key) : null;
    this.terms.set(id, { terms: full, salt: full.salt, proposal, adjudicator, custodian });
    return { id, terms: full, salt: full.salt, commitment: com, proposal, sealed_terms, submission };
  }

  // ---- accept ----------------------------------------------------------------

  /** `terms` = the full terms the proposer sent off-chain (with salt). */
  async accept(id, terms, { min_reputation } = {}) {
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
    // Learn early whether I even qualify, so I do not take on work the
    // custodian will refuse to fund. The custodian enforces this from the
    // proposer's actual grant at lock; this is the acceptor checking itself.
    if (p.min_reputation) {
      const { indexer, score: min } = p.min_reputation;
      const card = await this.reputationOf(this.email, indexer);
      const score = Math.round(Number(card.score ?? 0));
      if (score < min) throw new AgreementError("reputation_below_minimum", `${this.email} is rated ${score} at ${indexer}; the proposal requires ${min}`);
    }
    const adjudicator = await this.#adjudicatorFor(p);
    await this.#contractWarrant(id, totalMinor, currency, p.parties.proposer, adjudicator?.origin);
    // Register the account these earnings belong to. The grant's grantor is the
    // human, so this cannot be redirected by the agent — which it could be if
    // the acceptance simply declared an account.
    await this.#registerWithCustodian(p.custodian, id);
    const proposal_hash = "sha256:" + sha256hex(Buffer.from(canonicalize(p)));
    // No bundle: the chain attributes this acceptance to us. `min_reputation`
    // travels here because it is OUR requirement of the other side, and this
    // object is our signed statement of it.
    const acceptance = { v: 1, id, proposal_hash, ...(min_reputation ? { min_reputation: { indexer: (min_reputation.indexer || this.index).replace(/\/+$/, ""), score: Math.round(Number(min_reputation.score)) } } : {}) };
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
    const presentation = await this.#contractAssertion(id);
    // The payee's bundle: its grantor is the account the price is released to.
    const body = {
      presentation, proposal: p,
      price: st.terms.price, fees: st.terms.fees, contribution: p.contribution, sealed_terms,
    };
    return this.#custodian(p.custodian, "/api/escrow/lock", body);
  }

  // ---- deliver / settle / dispute ----------------------------------------------

  async #evidence(id, kind, extra = {}) {
    const ev = { v: 1, id, kind, by: this.email, at: nowS(), ...extra };
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
  /**
   * Receipt for a handoff: `content` (what was received) is hashed and checked
   * against the handoff's commitment — a mismatch is posted as contested with
   * the reason; `contested` posts a contest without content.
   */
  /**
   * Each obligation with its state, from the terms (local or public), the
   * evidence on chain and the clock: met | pending | late | blocked (its
   * dependency is unmet). Deadlines slide by a dependency's lateness. The
   * counterparty of a late obligation holds the dispute right.
   */
  // ---- evidence patterns (docs/specs/evidence-patterns.md) -----------------------

  /**
   * Input handoff: the party providing inputs commits to them on the ledger
   * (`kind: "handoff"`) and sends them off-chain; the receiver answers with
   * `acknowledgeReceipt`. `obligation` = index of the input obligation this
   * fulfils (optional when the terms have one input obligation).
   */
  /**
   * Receipt for a handoff: `content` (what was received) is hashed and checked
   * against the handoff's commitment — a mismatch is posted as contested with
   * the reason; `contested` posts a contest without content.
   */
  /**
   * Each obligation with its state, from the terms (local or public), the
   * evidence on chain and the clock: met | pending | late | blocked (its
   * dependency is unmet). Deadlines slide by a dependency's lateness. The
   * counterparty of a late obligation holds the dispute right.
   */
  /** Payer acknowledges delivery: evidence on chain, then the custodian releases. */
  /** Either party disputes: evidence on chain, freeze at the custodian, file with the adjudicator. */

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
    // Timings are fixed by the protocol, not negotiated: a window the proposer
    // chooses is a window an attacker tunes, and there are no policy-enforced
    // floors yet (browserid-pay-b3gq). `deriveObligations` still takes them as
    // arguments so the behaviour is testable without waiting days.
    return deriveObligations(terms.obligations || [], { evidence: evs, lock: !!lock, now: nowS(), parties: p.parties, roles: p.roles,
      responseWindow: this.windows.response, resolutionWindow: this.windows.resolution });
  }

  /** Payer acknowledges delivery: evidence on chain, then the custodian releases. */
  /**
   * Answer a delivery. Accepting releases the escrow; refusing does not, and
   * stops the clock so silence cannot later be read as acceptance.
   *
   * `reason` says which kind of refusal it is, because they are not the same
   * complaint: `not_received` is the deliverer's to cure by delivering again,
   * `not_met` is a disagreement about the work and belongs to the adjudicator.
   */
  async settle(id, { note, ok = true, reason } = {}) {
    const p = await this.#proposal(id);
    if (p.roles.payer !== this.email) throw new AgreementError("not_payer");
    if (!ok) {
      if (!["not_received", "not_met"].includes(reason || "")) {
        throw new AgreementError("bad_request", 'refusing needs reason "not_received" or "not_met"');
      }
      // Evidence only: nothing is released, and the escrow stays locked.
      return this.#evidence(id, "acknowledgement", { note, ok: false, reason });
    }
    const ev = await this.#evidence(id, "acknowledgement", { note, ok: true });
    const presentation = await this.#contractAssertion(id);
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
    const presentation = await this.#contractAssertion(id);
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

  /**
   * Agreement ids I am a party to. Asks the indexer's reverse index first, and
   * falls back to scanning `/agreements/` on chain — so an agent can follow its
   * own workflow with no indexer at all, just more slowly.
   */
  async mine() {
    try {
      const r = await this.http(`${this.index}/v1/agreements?party=${encodeURIComponent(this.email)}`);
      if (r.ok) {
        const j = await r.json();
        const ids = (j.results || []).map((x) => x.id || x.agreement).filter(Boolean);
        if (ids.length) return ids;
      }
    } catch {}
    const objs = await this.chain.list("/agreements/").catch(() => []);
    const ids = [];
    for (const o of objs) {
      if (o.id !== "proposal") continue;
      const pr = o.payload || {};
      const parties = [pr.parties?.proposer, pr.parties?.acceptor];
      if (!parties.includes(this.email)) continue;
      const m = /^\/agreements\/([^/]+)\//.exec(o.path || "");
      if (m) ids.push(m[1]);
    }
    return ids;
  }

  /**
   * The single next thing *I* should do, across every agreement I am party to.
   *
   * Read-only: it never consumes messages (that is `check_messages`, which
   * advances the read cursor) and never writes. When a step depends on
   * something that arrives off-chain — the full terms an acceptor needs — it
   * returns `check_messages` rather than guessing.
   *
   * Returns `{ action, agreement?, why, then? }`. `action: "none"` means there
   * is genuinely nothing to do, which is the normal answer while waiting on the
   * other side.
   */
  // ---- goals: what this agent is trying to achieve -------------------------

  /**
   * Register a goal. Everything else about an agent's work is reactive — this
   * is the one place it records what it set out to do, so a loop can pick the
   * thread back up on the next tick.
   *
   * `counterparty` may be unknown: a goal with nobody to ask is in `sourcing`,
   * which is a real state with a real next action, not an error.
   */
  addGoal({ kind = "errand", summary, counterparty = null, budget = null, deadline = null, terms = null, note = null, accept_if = null, escalate_if = null } = {}) {
    if (!summary) throw new AgreementError("bad_request", "a goal needs a summary");
    if (!["errand", "inbound"].includes(kind)) throw new AgreementError("bad_request", `unknown goal kind ${kind}`);
    const id = "goal_" + randomBytes(6).toString("base64url");
    // An INBOUND goal is a standing policy, not an errand: work comes to you,
    // so it has no counterparty to find, nothing to propose and no finish. It
    // never produces a step of its own — what it does is carry the criteria
    // for judging what arrives, which is otherwise remembered only in the
    // conversation and lost with it.
    const state = kind === "inbound" ? "standing" : counterparty ? "ready" : "sourcing";
    const g = { id, kind, summary, counterparty, budget, deadline, terms, note, accept_if, escalate_if, agreement: null, state, created_at: nowS(), updated_at: nowS() };
    this.goals.set(id, g);
    return g;
  }

  listGoals({ all = false } = {}) {
    const out = [];
    for (const [, g] of this.goals) if (all || !["done", "abandoned"].includes(g.state)) out.push(g);
    return out.sort((a, b) => a.created_at - b.created_at);
  }

  updateGoal(id, patch = {}) {
    const g = this.goals.get(id);
    if (!g) throw new AgreementError("no_goal", id);
    const next = { ...g, ...patch, id: g.id, kind: g.kind, created_at: g.created_at, updated_at: nowS() };
    // A standing policy has no pipeline to advance; only errands do.
    if (g.kind !== "inbound" && (!next.state || next.state === g.state)) {
      next.state = next.agreement ? "running" : next.counterparty ? "ready" : "sourcing";
    }
    this.goals.set(id, next);
    return next;
  }

  dropGoal(id, note = null) { return this.updateGoal(id, { state: "abandoned", note }); }

  /**
   * The single next thing *I* should do, across every goal I am pursuing and
   * every agreement I am party to.
   *
   * Read-only: it never consumes messages (that is `check_messages`, which
   * advances the read cursor) and never writes.
   *
   * Work in flight comes first — it has money committed — but a blocked
   * agreement never blocks a goal: each goal runs its own pipeline
   * (sourcing → ready → running → done), and anything not actionable is
   * reported in `also_waiting` rather than silently dropped.
   */
  async nextStep() {
    const ids = await this.mine().catch(() => []);
    const open = [];
    const byAgreement = new Map();
    for (const id of ids) {
      const st = await this.status(id).catch(() => null);
      if (!st) continue;
      byAgreement.set(id, st);
      if (!["settled", "ruled"].includes(st.stage)) open.push({ id, st });
    }
    // A goal whose agreement has finished is finished.
    for (const g of this.listGoals()) {
      if (g.kind === "inbound") continue;
      if (g.agreement && ["settled", "ruled"].includes(byAgreement.get(g.agreement)?.stage)) {
        this.updateGoal(g.id, { state: "done" });
      }
    }

    const waiting = [];
    // The standing policies for judging inbound work, attached to any decision
    // that calls for judgement so the rule is applied consistently instead of
    // being re-derived from the conversation each tick.
    const policy = this.listGoals()
      .filter((g) => g.kind === "inbound")
      .map((g) => ({ goal: g.id, summary: g.summary, accept_if: g.accept_if, escalate_if: g.escalate_if, note: g.note }));
    const decorate = (step) => (step.action === "accept" && policy.length ? { ...step, policy } : step);

    // 1. Agreements already running: real money is committed to these.
    for (const { id, st } of open) {
      const step = await this.#stepFor(id, st).catch((e) => ({ action: "none", why: `could not read ${id}: ${e.message}` }));
      if (step.action !== "none") return { agreement: id, ...decorate(step), ...(waiting.length ? { also_waiting: waiting } : {}) };
      waiting.push({ agreement: id, stage: st.stage, why: step.why });
    }
    // 2. Errands not yet attached to an agreement. A standing policy is never
    //    asked to source or propose — that was the bug it had to be lied to
    //    about (state faked to "running") to silence.
    for (const g of this.listGoals()) {
      if (g.kind === "inbound" || g.state === "running" || g.agreement) continue;
      const step = this.#stepForGoal(g);
      if (step.action !== "none") return { goal: g.id, ...step, ...(waiting.length ? { also_waiting: waiting } : {}) };
      waiting.push({ goal: g.id, state: g.state, why: step.why });
    }
    return { action: "none", why: waiting.length ? "everything is waiting on someone else" : "no goals and no open agreements", ...(waiting.length ? { also_waiting: waiting } : {}) };
  }

  /**
   * A goal's own pipeline. Each stage degrades to asking the human where the
   * automated path does not exist yet: there is no directory to search
   * (browserid-pay-h4mu) and no counter-offer protocol (browserid-pay-7bzw),
   * so "find a provider" and "agree terms" are conversations for now. The
   * pipeline shape is the same either way.
   */
  #stepForGoal(g) {
    if (!g.counterparty) {
      return { action: "find_counterparty", why: `"${g.summary}" has nobody to ask yet`,
               then: "there is no agent directory yet — ask the human who should do this, or message an agent you already know, then update the goal with the counterparty" };
    }
    if (!g.terms) {
      return { action: "agree_terms", why: `"${g.summary}": ${g.counterparty} is named but the terms are not settled`,
               then: "settle what, by when and for how much — with the human, or by messaging the counterparty — then update the goal with terms. Nothing binds until a proposal." };
    }
    return { action: "propose", why: `"${g.summary}": terms are settled with ${g.counterparty}`,
             terms: g.terms, counterparty: g.counterparty,
             then: "propose, passing this goal id so the agreement is linked to it, then send the counterparty the id and the FULL terms — they cannot accept without them" };
  }

  async #stepFor(id, st) {
    const p = await this.#proposal(id);
    const parties = p?.parties || {};
    const roles = p?.roles || {};
    const me = this.email;
    const iAmPayer = roles.payer === me;
    const iAmAcceptor = parties.acceptor === me;

    if (st.stage === "disputed") {
      // Prefer the judge's own case view (it carries the response window), but
      // fall back to chain evidence so a judge that is down or slow does not
      // leave the disputed party with nothing to do.
      const ev = st.evidence || [];
      const disputedBy = st.dispute?.disputed_by ?? ev.find((e) => e.kind === "dispute")?.by ?? null;
      const iResponded = ev.some((e) => e.kind === "response" && e.by === me);
      const mustRespond = st.dispute?.you_must_respond ?? (disputedBy != null && disputedBy !== me && !iResponded);
      if (mustRespond) {
        const by = st.dispute?.respond_by ? ` by ${st.dispute.respond_by}` : "";
        return { action: "respond", why: `disputed by ${disputedBy}; I must respond${by}`,
                 then: "reveal the delivered content so the judge can check it against my commitment" };
      }
      return { action: "none", why: "disputed; the judge rules once the response window closes" };
    }
    if (st.stage === "proposed") {
      if (!iAmAcceptor) return { action: "none", why: "proposed; waiting for the acceptor" };
      const terms = this.terms.get(id)?.terms || p?.terms;
      if (!terms) {
        return { action: "check_messages", why: "I am the acceptor but do not hold the full terms yet; they arrive off-chain" };
      }
      // Everything needed to judge whether to accept, so a standing policy
      // ("take what I can do alone, otherwise ask") can be applied without a
      // second call — the workflow says do one step per turn.
      const owed = (terms.obligations || []).filter((o) => o.by === me);
      return {
        action: "accept",
        why: `${parties.proposer} proposed this and I am the acceptor`,
        review: {
          from: parties.proposer,
          title: terms.title ?? null,
          description: terms.description ?? null,
          i_must: owed.map((o) => ({ what: o.what, kind: o.kind || "deliverable", deadline: o.deadline ?? null })),
          i_am_paid: roles.payee === me ? terms.price : null,
          i_pay: roles.payer === me ? terms.price : null,
          adjudicator: p?.adjudicator ?? null,
          expires: p?.expires ?? null,
        },
        then: "decide FIRST whether you can actually do what `i_must` asks; accept only then, and tell the proposer with send_message",
      };
    }
    // Funded is the custodian's escrow record, not the on-chain lock: the
    // custodian posts that object asynchronously, so the chain lags the truth.
    if (!st.escrow) {
      return iAmPayer
        ? { action: "fund", why: "accepted and not funded; I am the payer" }
        : { action: "none", why: "accepted; waiting for the payer to fund the escrow" };
    }
    {
      const obs = await this.obligations(id).catch(() => []);
      const mineUnmet = obs.filter((o) => o.by === me && o.state !== "met");
      const input = mineUnmet.find((o) => o.kind === "input" && o.state !== "handed_off");
      if (input) {
        return { action: "handoff", why: `my input obligation "${input.what}" is ${input.state}`,
                 obligation: input.index, then: "send the material itself with send_message; the handoff posts only a commitment" };
      }
      const toReceive = obs.find((o) => o.kind === "input" && o.by !== me && o.state === "handed_off");
      if (toReceive) {
        return { action: "acknowledge_receipt", why: `${toReceive.by} handed off "${toReceive.what}" and the receipt is outstanding`,
                 obligation: toReceive.index };
      }
      const deliverable = mineUnmet.find((o) => o.kind === "deliverable" && ["pending", "late"].includes(o.state));
      if (deliverable) {
        return { action: "deliver", why: `my deliverable "${deliverable.what}" is ${deliverable.state}`,
                 obligation: deliverable.index, then: "tell the other party with send_message" };
      }
      // A delivery awaiting my answer: my clock is running, and saying nothing
      // will be read as acceptance.
      const awaiting = obs.find((o) => ["delivered", "unanswered"].includes(o.state) && o.by !== me);
      if (awaiting && iAmPayer) {
        const overdue = awaiting.state === "unanswered";
        return {
          action: "settle",
          why: overdue
            ? `${awaiting.by} delivered "${awaiting.what}" and my window to answer has already passed`
            : `${awaiting.by} delivered "${awaiting.what}" and it is my turn to answer`,
          response_due: awaiting.response_due ?? null,
          ...(overdue ? { overdue: true } : {}),
          then: 'check what actually arrived. Accept to release the escrow; if nothing arrived call settle with ok:false and reason "not_received" so they can send it again; if it arrived but is wrong, reason "not_met".'
            + (overdue ? " You can still answer, and answering is far cheaper than a ruling — but they may go to the adjudicator at any point now." : ""),
        };
      }
      // My own delivery was refused: cure it by delivering again (which
      // restarts their clock), or dispute if the refusal is unfounded.
      const refused = obs.find((o) => o.state === "contested" && o.by === me);
      if (refused) {
        return refused.contest_reason === "not_received"
          ? { action: "deliver", why: `${other(me, parties)} says "${refused.what}" never arrived`, obligation: refused.index,
              then: "deliver again and SEND the content this time; that restarts their window" }
          : { action: "none", why: `${other(me, parties)} contested "${refused.what}" as not meeting the terms; agree a fix or dispute it` };
      }
      // The arrival argument ran out of time: nothing here will resolve it, and
      // no clock will release the escrow, so it takes an adjudicator.
      const stalled = obs.find((o) => o.state === "stalled" && o.by === me);
      if (stalled) {
        return { action: "dispute", why: `"${stalled.what}": ${other(me, parties)} has not acknowledged receipt and the time to settle that has run out`,
                 then: "the adjudicator sees the whole exchange — every delivery, every refusal and when each happened — and the escrowed adjudicator fee already covers it" };
      }
      // Nobody answered. Nothing releases the escrow on its own, so being paid
      // means asking the adjudicator.
      const unanswered = obs.find((o) => o.state === "unanswered" && o.by === me);
      if (unanswered && st.escrow?.state === "locked") {
        return { action: "dispute", why: `the delivery of "${unanswered.what}" went unanswered past the window`,
                 then: "open your delivery commitment for the adjudicator — an unopened commitment counts as undelivered — and the fee is already escrowed. Message them first if you have not: an answer is far cheaper than a ruling." };
      }
      const blocked = mineUnmet.find((o) => o.state === "blocked");
      if (blocked) return { action: "none", why: `my "${blocked.what}" is blocked on obligation ${blocked.depends_on}` };
      return { action: "none", why: "nothing outstanding on my side" };
    }
    return { action: "none", why: `stage ${st.stage}` };
  }

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
    // Only an ACCEPTING acknowledgement settles it. A refusal (`ok: false`) is
    // an acknowledgement too, and treating it as a settlement would hide a
    // still-open agreement from both parties.
    if (stage !== "none" && evidence.some((e) => e.payload?.kind === "acknowledgement" && e.payload?.ok !== false)) stage = "settled";
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
  return price + fees + contributionOf(terms.contribution, price);
}

/**
 * The contribution in minor units, for a price in minor units.
 *
 * `fn` combines the two operands: `max` (the usual floor — "1bp, or at least a
 * cent"), `min` (a ceiling on both), or absent for a single operand. Kept as
 * STRUCTURE rather than an expression string so a chain policy can pin `fn` and
 * bound each operand with plain value conditions — no parser in the policy
 * language. See the SBO Policy Specification, "Payload-field conditions".
 */
export function contributionOf(c, priceMinor) {
  if (!c) return 0;
  const pct = c.percentage == null ? null : Math.ceil((priceMinor * Number(c.percentage)) / 100);
  const abs = c.amount == null ? null : money(c.amount);
  if (pct == null && abs == null) return 0;
  if (pct == null) return abs;
  if (abs == null) return pct;
  if (c.fn === "max") return Math.max(abs, pct);
  if (c.fn === "min") return Math.min(abs, pct);
  // Both operands present with no `fn` is ambiguous — refuse rather than guess.
  throw new AgreementError("contribution_fn_required");
}

/**
 * The chain-enforced contribution floor, mirrored client-side: `max(1 cent, 1bp)`.
 * Kept in step with the root policy (`crates/genesis`, bean browserid-pay-er8j).
 */
export const CONTRIBUTION_FLOOR = { amount: "0.01", percentage: 0.01 };

/**
 * Normalize a contribution to the object that goes on chain and into the terms.
 *
 * The root policy requires `fn`, `amount` and `percentage` to all be PRESENT and
 * at or above the floor — a missing field fails closed and the proposal is
 * refused on chain. So all three are always emitted, and a caller asking for
 * less than the floor is raised to it rather than silently rejected later by the
 * daemon.
 */
export function normalizeContribution(c) {
  if (c?.fn != null && c.fn !== "max") {
    // `min` would let a proposal pay the smaller operand, so the chain pins
    // `max`. Fail here rather than at submit, where the error is opaque.
    throw new AgreementError("contribution_fn_unsupported");
  }
  const amountMinor = Math.max(c?.amount != null ? money(c.amount) : 0, money(CONTRIBUTION_FLOOR.amount));
  const percentage = Math.max(Number(c?.percentage ?? 0), CONTRIBUTION_FLOOR.percentage);
  return { recipient: c?.recipient ?? null, fn: "max", amount: fmt(amountMinor), percentage };
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

// ---- evidence patterns: pure helpers ---------------------------------------------

/** `sha256:<hex>` over a string, or the canonical JSON of an object. */
export function contentCommitment(content) {
  return "sha256:" + sha256hex(Buffer.from(typeof content === "string" ? content : canonicalize(content)));
}

function defaultManifest(content) {
  if (Array.isArray(content)) return { count: content.length, items: content.slice(0, 64).map((c, i) => ({ name: typeof c === "string" ? c.slice(0, 120) : c?.name || c?.title || `item ${i + 1}`, size: Buffer.byteLength(typeof c === "string" ? c : canonicalize(c)) })) };
  if (content && typeof content === "object") return { count: Object.keys(content).length, items: Object.keys(content).slice(0, 64).map((k) => ({ name: k, size: Buffer.byteLength(canonicalize(content[k])) })) };
  return { count: 1, items: [{ name: "content", size: Buffer.byteLength(String(content)) }] };
}

/**
 * Obligation states (docs/specs/evidence-patterns.md "Milestone deadlines").
 * Pure: `terms.obligations` × evidence × clock. Exported for tests.
 *
 * kind (default "deliverable"): "input" is met by a receipt (ok) for a handoff
 * of that obligation (a handoff with no receipt is "handed_off"); "deliverable"
 * by a delivery from `by`; "payment" by the escrow lock. `depends_on` (index)
 * blocks the obligation until its dependency is met, and slides its deadline
 * by the dependency's lateness (met after its own deadline).
 */
function other(me, parties = {}) {
  return [parties.proposer, parties.acceptor].filter(Boolean).find((x) => x !== me) || "the other party";
}

export function deriveObligations(obligations, { evidence = [], lock = false, now = nowS(), parties = {}, roles = {}, responseWindow = DEFAULT_RESPONSE_WINDOW, resolutionWindow = null } = {}) {
  const respondSecs = windowSecs(responseWindow) ?? windowSecs(DEFAULT_RESPONSE_WINDOW);
  const resolveSecs = windowSecs(resolutionWindow) ?? respondSecs * DEFAULT_RESOLUTION_MULTIPLE;
  const ofObligation = (kind, i, by) => evidence.filter((e) => e.kind === kind && (e.obligation === undefined ? (by === undefined || e.by === by) : e.obligation === i));
  const out = obligations.map((o, i) => ({ index: i, by: o.by, what: o.what, kind: o.kind || "deliverable", deadline: o.deadline ?? null, depends_on: o.depends_on ?? null }));
  // First pass: when was each obligation met (writer-claimed `at`, else unknown).
  const metAt = (o) => {
    const inputs = ofObligation("handoff", o.index, o.by);
    if (o.kind === "input") {
      const receipts = evidence.filter((e) => e.kind === "receipt" && e.ok === true && inputs.some((h) => h.id === e.handoff));
      if (receipts.length) return { met: true, at: Math.min(...receipts.map((r) => r.at ?? Infinity)) };
      if (inputs.length) return { met: false, handed_off: true, contested: evidence.some((e) => e.kind === "receipt" && e.ok === false && inputs.some((h) => h.id === e.handoff)) };
      return { met: false };
    }
    if (o.kind === "payment") return { met: lock, at: null };
    // A delivery COMMITMENT is not yet a delivery: it starts the recipient's
    // response window. Silence past it is deemed acceptance; an explicit
    // refusal contests it; a later delivery is a cure and restarts the clock.
    // Only the LATEST delivery and any answer after it matter.
    // Order by the LEDGER, not by wall clock: `at` is writer-claimed and has
    // second resolution, so a cure and the refusal it answers routinely share a
    // timestamp. Position in the (chain-ordered) evidence list breaks ties, and
    // block number leads when present.
    const rank = (e) => [e.block ?? Number.MAX_SAFE_INTEGER, e.seq];
    const ordered = evidence.map((e, seq) => ({ ...e, seq }));
    const after = (a, b) => { const [x1, y1] = rank(a), [x2, y2] = rank(b); return x1 !== x2 ? x1 > x2 : y1 > y2; };
    const deliveries = ordered.filter((e) => e.kind === "delivery" && (o.by === undefined || e.by === o.by));
    if (!deliveries.length) return { met: false };
    const last = deliveries[deliveries.length - 1];
    const answer = ordered
      .filter((e) => e.kind === "acknowledgement" && e.by !== o.by && after(e, last))
      .pop();
    // Measured from the FIRST delivery and never restarted, so repeated
    // "I never got it" cannot extend the argument for ever.
    const absoluteDue = (deliveries[0].at ?? now) + resolveSecs;
    if (answer) {
      // An acknowledgement with no `ok` is an acceptance: that is what every
      // acknowledgement meant before this field existed.
      if (answer.ok !== false) return { met: true, at: answer.at ?? last.at ?? Infinity };
      const reason = answer.reason || "not_met";
      // `not_met` ACKNOWLEDGES receipt and argues about the work, so the
      // arrival question is closed and this clock stops. Only an unresolved
      // arrival keeps running against it.
      if (reason === "not_met") {
        return { met: false, delivered: true, contested: true, reason, at: answer.at ?? null };
      }
      return {
        met: false, delivered: true, contested: true, reason, at: answer.at ?? null,
        absolute_due: absoluteDue, ...(now > absoluteDue ? { stalled: true } : {}),
      };
    }
    const due = (last.at ?? now) + respondSecs;
    // A lapsed window is missing information, not a completed job. Treating
    // silence as acceptance let a withholder wait out an inattentive recipient
    // and take the money without ever facing an adjudicator; now nobody wins by
    // default and somebody has to ask for a decision.
    if (now > due) return { met: false, delivered: true, unanswered: true, response_due: due, absolute_due: absoluteDue };
    return { met: false, delivered: true, response_due: due, absolute_due: absoluteDue };
  };
  const status = out.map(metAt);
  // Second pass: effective deadlines (slid by a dependency's lateness) and state.
  const effectiveDeadline = (o) => {
    if (o.deadline == null) return null;
    if (o.depends_on == null || !out[o.depends_on]) return o.deadline;
    const dep = out[o.depends_on], ds = status[o.depends_on];
    if (!ds.met || dep.deadline == null || !Number.isFinite(ds.at)) return o.deadline;
    return o.deadline + Math.max(0, ds.at - dep.deadline);
  };
  const other = (who) => {
    const ps = [parties.proposer, parties.acceptor].filter(Boolean);
    return ps.find((x) => x !== who) || null;
  };
  return out.map((o, i) => {
    const st = status[i];
    const deadline = effectiveDeadline(o);
    let state;
    if (st.met) state = "met";
    // `stalled` is a contested arrival that ran out of time, so it must be
    // tested BEFORE the plain contested case or it can never surface.
    else if (st.stalled) state = "stalled";
    else if (st.contested) state = "contested";
    else if (st.unanswered) state = "unanswered";
    else if (st.delivered) state = "delivered";
    else if (o.depends_on != null && !status[o.depends_on]?.met) state = "blocked";
    else if (st.handed_off) state = "handed_off";
    else if (deadline != null && now > deadline) state = "late";
    else state = "pending";
    return { ...o, effective_deadline: deadline, state, met_at: st.met && Number.isFinite(st.at) ? st.at : null,
      ...(st.response_due ? { response_due: st.response_due } : {}),
      ...(st.absolute_due ? { absolute_due: st.absolute_due } : {}),
      ...(st.reason ? { contest_reason: st.reason } : {}),
      // A contested delivery is the deliverer's to cure, so the dispute right
      // sits with the party who raised it; `late` is unchanged.
      // `stalled` is the deliverer's to escalate — the recipient will not
      // resolve it and nothing else will. (Either party may in fact dispute;
      // this names who has reason to.)
      // `unanswered` deliberately names nobody: silence is not yet anyone's
      // fault, and which way it goes is what the dispute is for. Either party
      // may raise one — the deliverer to be paid, the recipient to be refunded.
      dispute_right: state === "stalled" ? o.by : state === "late" || state === "contested" ? other(o.by) : null };
  });
}
