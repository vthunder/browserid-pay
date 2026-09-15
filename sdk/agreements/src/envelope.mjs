// SBO envelope builder (Wire Format spec v0.5, canonical header order), for
// device-model attributed writes: `Auth-Cert` carries the four-object
// presentation, `Public-Key` is the ACCESS key that signed it (ed25519:<hex>),
// and the envelope signature is by that same key. The daemon's device-model
// attribution (sbo-core device_attribution.rs) binds the two.
import { createHash } from "node:crypto";

const ORDER = [
  "SBO-Version", "Action", "Path", "ID", "Type", "Content-Type", "Content-Encoding",
  "Content-Length", "Content-Hash", "Attestation", "Content-Schema", "Creator", "HLC",
  "New-ID", "New-Owner", "New-Path", "Object-Path", "Origin", "Owner", "Policy-Ref", "Prev",
  "Proof", "Proof-Type", "Registry-Path", "Related", "Auth-Cert", "Auth-Evidence",
  "Auth-Warrant", "Public-Key", "Signature",
];

export const pubkeyHex = (raw32) => "ed25519:" + Buffer.from(raw32).toString("hex");

/**
 * Build the signed wire bytes of one message.
 * @param {object} m { action, path, id, type?, contentType?, schema?, payload (Buffer|string), owner?, authCert, publicKeyHex, sign: (Buffer)=>Uint8Array }
 */
export function buildEnvelope(m) {
  const payload = m.payload === undefined ? null : Buffer.from(m.payload);
  const h = {
    "SBO-Version": "0.5",
    Action: m.action || "post",
    Path: m.path,
    ID: m.id,
    Type: m.type || "object",
  };
  if (!h.Path.endsWith("/")) throw new Error("Path must end with /");
  if (h.ID.includes("/")) throw new Error("ID must not contain /");
  if (payload) {
    h["Content-Type"] = m.contentType || "application/json";
    h["Content-Length"] = String(payload.length);
    h["Content-Hash"] = "sha256:" + createHash("sha256").update(payload).digest("hex");
  }
  if (m.schema) h["Content-Schema"] = m.schema;
  if (m.owner) h.Owner = m.owner;
  if (m.hlc) h.HLC = m.hlc;
  if (m.authCert) h["Auth-Cert"] = m.authCert;
  h["Public-Key"] = m.publicKeyHex;
  for (const [k, v] of Object.entries(h)) {
    if (/[\r\n]/.test(String(v))) throw new Error(`header ${k} contains a newline`);
  }
  const canonical = ORDER.filter((k) => k in h && k !== "Signature").map((k) => `${k}: ${h[k]}\n`).join("") + "\n";
  const sig = Buffer.from(m.sign(Buffer.from(canonical, "utf8"))).toString("hex");
  h.Signature = sig;
  const head = ORDER.filter((k) => k in h).map((k) => `${k}: ${h[k]}\n`).join("") + "\n";
  return { wire: payload ? Buffer.concat([Buffer.from(head, "utf8"), payload]) : Buffer.from(head, "utf8"), headers: h };
}

/** Parse one wire message back into { headers, payload } (for tests / fakes). */
export function parseEnvelope(wire) {
  const buf = Buffer.from(wire);
  const sep = buf.indexOf("\n\n");
  if (sep < 0) throw new Error("no header terminator");
  const headers = {};
  for (const line of buf.subarray(0, sep).toString("utf8").split("\n")) {
    const i = line.indexOf(": ");
    if (i < 0) throw new Error(`bad header line: ${line}`);
    headers[line.slice(0, i)] = line.slice(i + 2);
  }
  const len = Number(headers["Content-Length"] || 0);
  const payload = buf.subarray(sep + 2, sep + 2 + len);
  return { headers, payload };
}
