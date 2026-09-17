// Test doubles: a fake agent (ed25519 keys, self-made "presentations" that
// carry the identity in a parseable warrant), a fake verifier that reads
// them back, and in-process custodian + judge HTTP servers.
import { createServer } from "node:http";
import { KeyPair } from "@browserid-ng/agent";

const b64uJson = (o) => Buffer.from(JSON.stringify(o)).toString("base64url");
const jwsLike = (claims) => `${b64uJson({ alg: "EdDSA" })}.${b64uJson(claims)}.sig`;

/** Looks like a DeviceAgent to the SDK. `grants` are recorded so tests can
 *  assert what the human would have been asked to sign. */
export class FakeAgent {
  constructor(email) {
    this.email = email;
    this.deviceCert = jwsLike({ typ: "browserid-device-cert-v1", identities: [email] });
    this.grants = new Map(); // audience -> scopes
    this.requested = [];
    this.cards = []; // one entry per consent card (bundled grants)
  }
  warrantedAudiences() { return [...this.grants.keys()]; }
  addGrant(pair) {
    const claims = JSON.parse(Buffer.from(pair.split("~")[0].split(".")[1], "base64url").toString());
    this.grants.set(claims.audience, claims.scopes);
  }
  grant(audience, scopes) { this.grants.set(audience, scopes); }
  #presentation(audience) {
    const scopes = this.grants.get(audience);
    if (!scopes) { const e = new Error(`no warrant for ${audience}`); e.name = "NoWarrantError"; throw e; }
    const warrant = jwsLike({ typ: "browserid-warrant-v2", grantor: this.email, grantee: this.email, audience, scopes });
    return `${jwsLike({ typ: "access" })}~${jwsLike({ aud: audience })}~${warrant}~${jwsLike({ typ: "config" })}`;
  }
  async assertionFor(audience) { return this.#presentation(audience); }
  async assertionWithAccessKey(audience) {
    const accessKey = KeyPair.generate();
    return { presentation: this.#presentation(audience), accessKey, accessCert: "cert" };
  }
  /** obtainWarrant hook: auto-approve, record the ask. */
  obtain = async (audience, scopes) => { this.requested.push({ audience, scopes }); this.grants.set(audience, scopes); };
  /** obtainWarrants hook: ONE card carrying every grant; auto-approve, record the card. */
  obtainAll = async (grants) => { this.cards.push(grants); for (const g of grants) this.grants.set(g.audience, g.scopes); };
}

/** Reads the fake presentation back: identity + scopes from its warrant. */
export async function fakeVerify(presentation, audience) {
  try {
    const w = JSON.parse(Buffer.from(presentation.split("~")[2].split(".")[1], "base64url").toString());
    if (w.audience !== audience) return { ok: false, reason: `audience ${w.audience} != ${audience}` };
    return { ok: true, email: w.grantor, grantee: w.grantee, scopes: w.scopes };
  } catch (e) { return { ok: false, reason: String(e) }; }
}

/** Minimal custodian + judge in one process; records calls. */
export async function fakeServers() {
  const calls = [];
  const escrows = new Map();
  const srv = createServer(async (req, res) => {
    const chunks = [];
    for await (const c of req) chunks.push(c);
    const body = chunks.length ? JSON.parse(Buffer.concat(chunks).toString()) : null;
    calls.push({ method: req.method, url: req.url, body });
    const json = (code, obj) => { res.writeHead(code, { "content-type": "application/json" }); res.end(JSON.stringify(obj)); };
    let m;
    // The acceptor registers which account its earnings belong to; the real
    // custodian keeps the mapping so it can pay a payee who is offline.
    if (req.url === "/api/escrow/register") return json(200, { registered: true });
    if (req.url === "/api/escrow/lock") {
      const id = body.proposal.id;
      escrows.set(id, { id, state: "locked", sealed_terms: body.sealed_terms, price: body.price });
      return json(200, { lock: { v: 1, id, locked_at: 1 }, state: "locked" });
    }
    if ((m = /^\/api\/escrow\/([^/]+)\/(acknowledge|dispute)$/.exec(req.url))) {
      const e = escrows.get(m[1]);
      if (!e) return json(404, { error: "unknown_escrow" });
      e.state = m[2] === "acknowledge" ? "released" : "disputed";
      return json(200, { state: e.state, sealed_terms: e.sealed_terms });
    }
    if ((m = /^\/api\/escrow\/([^/]+)$/.exec(req.url))) {
      const e = escrows.get(m[1]);
      return e ? json(200, e) : json(404, { error: "unknown_escrow" });
    }
    if (req.url === "/api/dispute") return json(200, { filed: true, contract_id: body.contract_id });
    json(404, { error: "no_route" });
  });
  await new Promise((r) => srv.listen(0, "127.0.0.1", r));
  const origin = `http://127.0.0.1:${srv.address().port}`;
  return { origin, calls, escrows, close: () => new Promise((r) => srv.close(r)) };
}
