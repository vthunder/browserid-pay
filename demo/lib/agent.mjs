// An agent for the local demo: the real `@browserid-ng/agent` DeviceAgent
// (mints access certs, builds presentations, signs envelopes) holding a
// credential the demo IdP issued. The human's consent card is replaced by
// `demo-idp`'s `/demo/grant`, which signs the warrant as the grantor.
import { DeviceAgent } from "@browserid-ng/agent";

async function postJson(url, body) {
  const r = await fetch(url, { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(body) });
  const j = await r.json().catch(() => ({}));
  if (!r.ok) throw new Error(`${url} → ${r.status}: ${j.error || JSON.stringify(j)}`);
  return j;
}

export class DemoAgent {
  /** @param {string} idp demo-idp origin; @param {string} email identity */
  static async create(idp, email) {
    const cred = await postJson(`${idp}/demo/credential`, { identity: email });
    const agent = new DeviceAgent({ device_key: cred.device_key, agent_device_cert: cred.agent_device_cert, idp: cred.idp, identity: email });
    return new DemoAgent(idp, agent);
  }

  constructor(idp, agent) {
    this.idp = idp;
    this.agent = agent;
    this.asked = []; // what a human would have been shown
  }

  get email() { return this.agent.email; }

  /** The SDK's `obtainWarrant` hook: "the human approves" = the demo IdP signs. */
  obtain = async (audience, scopes) => {
    this.asked.push({ audience, scopes });
    const g = await postJson(`${this.idp}/demo/grant`, { grantor: this.email, grantee: this.email, audience, scopes, holder: this.agent.holder });
    this.agent.addGrant(g.pair);
  };

  /** A presentation for an arbitrary audience (e.g. the judge), granting on the fly. */
  async presentationFor(audience, scopes = ["login"]) {
    if (!this.agent.warrantedAudiences().includes(audience)) await this.obtain(audience, scopes);
    return this.agent.assertionFor(audience);
  }
}

/** The SDK's `verify` hook, answered by the demo IdP (it pins its own key). */
export const verifyVia = (idp) => async (presentation, audience) => postJson(`${idp}/demo/verify`, { presentation, audience });
