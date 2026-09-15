// RFC 8785 (JSON Canonicalization Scheme): sorted object keys (by UTF-16 code
// units), no whitespace, ES6 number serialization, JSON string escaping. Enough
// for agreement terms (strings, integers, decimal strings, small floats).
import { createHash } from "node:crypto";

export function canonicalize(value) {
  if (value === null || typeof value === "boolean") return JSON.stringify(value);
  if (typeof value === "number") {
    if (!Number.isFinite(value)) throw new TypeError("JCS: non-finite number");
    return Object.is(value, -0) ? "0" : JSON.stringify(value);
  }
  if (typeof value === "string") return JSON.stringify(value);
  if (Array.isArray(value)) return "[" + value.map((v) => canonicalize(v === undefined ? null : v)).join(",") + "]";
  if (typeof value === "object") {
    const keys = Object.keys(value).filter((k) => value[k] !== undefined).sort(cmpUtf16);
    return "{" + keys.map((k) => JSON.stringify(k) + ":" + canonicalize(value[k])).join(",") + "}";
  }
  throw new TypeError(`JCS: cannot serialize ${typeof value}`);
}

function cmpUtf16(a, b) {
  const n = Math.min(a.length, b.length);
  for (let i = 0; i < n; i++) {
    const d = a.charCodeAt(i) - b.charCodeAt(i);
    if (d) return d;
  }
  return a.length - b.length;
}

export const sha256hex = (bytes) => createHash("sha256").update(bytes).digest("hex");

/** `sha256:<hex>` over the canonical form of `value`. */
export function commitment(value) {
  return "sha256:" + sha256hex(Buffer.from(canonicalize(value), "utf8"));
}
