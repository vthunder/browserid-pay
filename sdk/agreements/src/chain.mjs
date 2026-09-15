// Chain access through an sbo-daemon: reads (`/v1/object`, `/v1/list`) and
// writes (`POST /v1/submit`), always with `repo=`. The daemon holds the repo's
// TurboDA credentials and refreshes issuer DNSSEC evidence ahead of any write
// that needs it, so agents carry no chain keys of their own.
import { parseEnvelope } from "./envelope.mjs";

export const DEFAULTS = {
  daemon: "https://da.sandmill.org",
  repo: "sbo+raw://avail:turing:530/",
};

export class DaemonChain {
  constructor({ daemon = DEFAULTS.daemon, repo = DEFAULTS.repo, http = fetch } = {}) {
    this.daemon = daemon.replace(/\/+$/, "");
    this.repo = repo;
    this.http = http;
  }

  #q(params) {
    const u = new URLSearchParams({ ...params, repo: this.repo });
    return u.toString();
  }

  /** One confirmed object → { payload (parsed JSON when JSON), headers?, raw } or null. */
  async get(path, id) {
    const r = await this.http(`${this.daemon}/v1/object?${this.#q({ path, id })}`);
    if (r.status === 404) return null;
    if (!r.ok) throw new Error(`daemon /v1/object ${r.status}`);
    return normalizeView(await r.json());
  }

  /** Confirmed objects under `prefix` (each normalized like `get`). */
  async list(prefix) {
    const r = await this.http(`${this.daemon}/v1/list?${this.#q({ prefix })}`);
    if (!r.ok) throw new Error(`daemon /v1/list ${r.status}`);
    const j = await r.json();
    const items = Array.isArray(j) ? j : j.objects || j.items || [];
    return items.map(normalizeView);
  }

  /** Submit signed wire bytes through the daemon. Returns the submission id. */
  async submit(wire) {
    const r = await this.http(`${this.daemon}/v1/submit?${this.#q({})}`, {
      method: "POST",
      headers: { "content-type": "application/octet-stream" },
      body: wire,
    });
    if (!r.ok) throw new Error(`daemon /v1/submit ${r.status}: ${await r.text().catch(() => "")}`);
    const j = await r.json();
    if (j.evidence_refreshed?.length) this.lastEvidenceRefreshed = j.evidence_refreshed;
    return j.submission_id || j.hash || "submitted";
  }
}

// The daemon's object view carries the payload in one of a few shapes across
// versions; take the first that looks like content, and parse JSON when it is.
export function normalizeView(v) {
  if (!v || typeof v !== "object") return null;
  // sbo-daemon (2026-09): `value` is the parsed JSON, `payload_text` the raw
  // text; older/other shapes below.
  let payload = v.value ?? v.payload ?? v.payload_text ?? v.content ?? v.object?.payload ?? v.object?.content ?? null;
  if (typeof payload === "string") {
    if (v.encoding === "base64" || v.payload_encoding === "base64") payload = Buffer.from(payload, "base64").toString("utf8");
    try { payload = JSON.parse(payload); } catch {}
  }
  const path = v.path ?? v.object?.path ?? null;
  const id = v.id ?? v.object?.id ?? null;
  const author = v.attributed_email ?? v.owner_ref ?? v.owner ?? v.creator ?? v.object?.owner ?? null;
  return { path, id, payload, author, block: v.block ?? v.confirmed_block ?? null, raw: v };
}

/** In-memory chain for tests: ordered, first-write-wins per (path,id). */
export class MemoryChain {
  constructor() { this.objects = []; this.submissions = 0; }
  #key(path, id) { return `${path}${id}`; }
  async get(path, id) {
    const k = this.#key(path, id);
    return this.objects.find((o) => this.#key(o.path, o.id) === k) || null;
  }
  async list(prefix) { return this.objects.filter((o) => o.path.startsWith(prefix)); }
  async submit(wire) {
    const { headers, payload } = parseEnvelope(wire);
    const k = this.#key(headers.Path, headers.ID);
    this.submissions++;
    if (this.objects.some((o) => this.#key(o.path, o.id) === k)) return `dup-${this.submissions}`; // slot taken: disregarded
    let parsed = payload.toString("utf8");
    try { parsed = JSON.parse(parsed); } catch {}
    const author = headers.Owner || (headers["Auth-Cert"] ? authorFromPresentation(headers["Auth-Cert"]) : headers["Public-Key"]);
    this.objects.push({ path: headers.Path, id: headers.ID, payload: parsed, author, block: this.objects.length + 1, headers, raw: null });
    return `sub-${this.submissions}`;
  }
  /** Put an object directly (fixtures). */
  put(path, id, payload, author = "sys") { this.objects.push({ path, id, payload, author, block: this.objects.length + 1, headers: {}, raw: null }); }
}

function authorFromPresentation(p) {
  try {
    const warrant = p.split("~")[2];
    const claims = JSON.parse(Buffer.from(warrant.split(".")[1], "base64url").toString("utf8"));
    return claims.grantee || claims.agent || null;
  } catch { return null; }
}
