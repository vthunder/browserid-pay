// A stub sbo-daemon for the local demo: the read routes the SDK and the judge
// use (`/v1/object`, `/v1/list`, always with `repo=`), plus `POST /v1/submit`
// that accepts a signed wire message and stores it — first write wins per
// (path, id), chain order = arrival order. No signature checks: the custodian
// and judge verify presentations themselves; the stub only has to behave
// like an honest indexer of what was posted.
import { createServer } from "node:http";
import { parseEnvelope } from "@browserid-ng/agreements";

const b64uJson = (s) => JSON.parse(Buffer.from(s, "base64url").toString("utf8"));

export class StubDaemon {
  constructor() { this.objects = []; this.server = null; this.origin = null; }

  put(path, id, value, creator = "sys", schema = null) {
    if (this.objects.some((o) => o.path === path && o.id === id)) return false;
    this.objects.push({ path, id, creator, value, content_schema: schema, block: 100 + this.objects.length, confirmed: true });
    return true;
  }

  get(path, id) { return this.objects.find((o) => o.path === path && o.id === id) || null; }

  /** Store one wire message. Creator = Owner, else the presentation's grantee, else the key. */
  submit(wire) {
    const { headers, payload } = parseEnvelope(wire);
    let creator = headers.Owner || null;
    if (!creator && headers["Auth-Cert"]) {
      try { creator = b64uJson(headers["Auth-Cert"].split("~")[2].split(".")[1]).grantee; } catch {}
    }
    if (!creator) creator = headers["Public-Key"];
    let value = payload.toString("utf8");
    try { value = JSON.parse(value); } catch {}
    const ok = this.put(headers.Path, headers.ID, value, creator, headers["Content-Schema"] || null);
    return { ok, path: headers.Path, id: headers.ID, creator };
  }

  #view(o) {
    return { path: o.path, id: o.id, creator: o.creator, value: o.value, payload: o.value, content_schema: o.content_schema, block: o.block, confirmed: true };
  }

  async listen(port = 0) {
    this.server = createServer(async (req, res) => {
      const u = new URL(req.url, "http://x");
      const json = (code, obj) => { res.writeHead(code, { "content-type": "application/json" }); res.end(JSON.stringify(obj)); };
      if (req.method === "GET" && u.pathname === "/health") return json(200, { ok: true });
      if (req.method === "GET" && u.pathname === "/v1/object") {
        const o = this.get(u.searchParams.get("path"), u.searchParams.get("id"));
        return o ? json(200, this.#view(o)) : json(404, { error: "not found" });
      }
      if (req.method === "GET" && u.pathname === "/v1/list") {
        const prefix = u.searchParams.get("prefix") || "";
        return json(200, this.objects.filter((o) => o.path.startsWith(prefix)).map((o) => this.#view(o)));
      }
      if (req.method === "POST" && u.pathname === "/v1/submit") {
        const chunks = [];
        for await (const c of req) chunks.push(c);
        try {
          const r = this.submit(Buffer.concat(chunks));
          return json(200, { submission_id: `stub-${this.objects.length}`, ...r });
        } catch (e) { return json(400, { error: String(e.message || e) }); }
      }
      json(404, { error: "no route" });
    });
    await new Promise((r) => this.server.listen(port, "127.0.0.1", r));
    this.origin = `http://127.0.0.1:${this.server.address().port}`;
    return this.origin;
  }

  close() { return new Promise((r) => (this.server ? this.server.close(r) : r())); }
}
