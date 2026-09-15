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
  /**
   * An agent delegated by a human: the agent identity is `<local>+agent@<domain>`
   * (a distinct grantee), the human is the grantor of every warrant it holds
   * and the account the custodian debits/credits — the live shape.
   * @param {string} idp demo-idp origin; @param {string} human the human's identity
   */
  static async create(idp, human) {
    const [local, domain] = human.split("@");
    const email = `${local}+agent@${domain}`;
    const mkAgent = async (identity) => {
      const cred = await postJson(`${idp}/demo/credential`, { identity });
      return new DeviceAgent({ device_key: cred.device_key, agent_device_cert: cred.agent_device_cert, idp: cred.idp, identity });
    };
    return new DemoAgent(idp, await mkAgent(email), await mkAgent(human), human);
  }

  constructor(idp, agent, humanAgent, human) {
    this.idp = idp;
    this.agent = agent;          // the delegated agent (parties, chain writes)
    this.humanAgent = humanAgent; // the human's own device (ledger sign-in)
    this.human = human;
    this.asked = []; // what the human would have been shown
  }

  get email() { return this.agent.email; }

  /** The SDK's `obtainWarrant` hook: "the human approves" = the demo IdP signs, grantor = the human. */
  obtain = async (audience, scopes) => {
    this.asked.push({ audience, scopes });
    const g = await postJson(`${this.idp}/demo/grant`, { grantor: this.human, grantee: this.email, audience, scopes, holder: this.agent.holder });
    this.agent.addGrant(g.pair);
  };

  /** The agent's presentation for an arbitrary audience (e.g. the judge), granting on the fly. */
  async presentationFor(audience, scopes = ["login"]) {
    if (!this.agent.warrantedAudiences().includes(audience)) await this.obtain(audience, scopes);
    return this.agent.assertionFor(audience);
  }

  /** The HUMAN's own login presentation (self-grant) — for the ledger page. */
  async humanPresentationFor(audience) {
    if (!this.humanAgent.warrantedAudiences().includes(audience)) {
      const g = await postJson(`${this.idp}/demo/grant`, { grantor: this.human, grantee: this.human, audience, scopes: ["login"], holder: this.humanAgent.holder });
      this.humanAgent.addGrant(g.pair);
    }
    return this.humanAgent.assertionFor(audience);
  }
}

/** The SDK's `verify` hook, answered by the demo IdP (it pins its own key). */
export const verifyVia = (idp) => async (presentation, audience) => postJson(`${idp}/demo/verify`, { presentation, audience });
