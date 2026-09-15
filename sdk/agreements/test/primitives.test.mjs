import test from "node:test";
import assert from "node:assert/strict";
import { createPublicKey, verify as edVerify } from "node:crypto";
import { canonicalize, commitment, seal, open, generateSealingKey, buildEnvelope, parseEnvelope, pubkeyHex } from "../index.mjs";
import { KeyPair } from "@browserid-ng/agent";

test("JCS: sorted keys, no whitespace, numbers", () => {
  assert.equal(canonicalize({ b: 1, a: [true, null, "x"], c: { z: 1.5, y: -0 } }), '{"a":[true,null,"x"],"b":1,"c":{"y":0,"z":1.5}}');
  assert.equal(canonicalize({ "é": 1, e: 2 }), '{"e":2,"é":1}');
  assert.equal(canonicalize({ a: undefined, b: 1 }), '{"b":1}');
  assert.match(commitment({ a: 1 }), /^sha256:[0-9a-f]{64}$/);
  assert.equal(commitment({ a: 1, b: 2 }), commitment({ b: 2, a: 1 }));
});

test("sealed-x25519-v1 round trip and tamper", () => {
  const k = generateSealingKey();
  const boxed = seal("hello terms", k.publicKey);
  assert.equal(open(boxed, k.secretKey).toString(), "hello terms");
  assert.notEqual(seal("hello terms", k.publicKey), boxed, "fresh ephemeral key per seal");
  const other = generateSealingKey();
  assert.throws(() => open(boxed, other.secretKey));
  const bad = Buffer.from(boxed, "base64url"); bad[40] ^= 1;
  assert.throws(() => open(bad.toString("base64url"), k.secretKey));
});

test("envelope: canonical header order, hash, signature verifies", () => {
  const kp = KeyPair.generate();
  const { wire, headers } = buildEnvelope({
    action: "post", path: "/agreements/agr_x/", id: "proposal", payload: Buffer.from('{"v":1}'),
    schema: "agreement.proposal.v1", authCert: "a~b~c~d", publicKeyHex: pubkeyHex(kp.publicRaw),
    sign: (b) => kp.signRaw(b),
  });
  const text = wire.toString("utf8");
  const lines = text.split("\n\n")[0].split("\n");
  assert.deepEqual(lines.map((l) => l.split(":")[0]), [
    "SBO-Version", "Action", "Path", "ID", "Type", "Content-Type", "Content-Length", "Content-Hash",
    "Content-Schema", "Auth-Cert", "Public-Key", "Signature",
  ]);
  assert.equal(headers["Content-Length"], "7");
  assert.equal(text.endsWith('\n\n{"v":1}'), true);
  // Signature covers all headers but Signature, plus the blank line.
  const canonical = lines.filter((l) => !l.startsWith("Signature:")).join("\n") + "\n\n";
  const spki = Buffer.concat([Buffer.from("302a300506032b6570032100", "hex"), Buffer.from(kp.publicRaw)]);
  const ok = edVerify(null, Buffer.from(canonical), createPublicKey({ key: spki, format: "der", type: "spki" }), Buffer.from(headers.Signature, "hex"));
  assert.equal(ok, true);
  const back = parseEnvelope(wire);
  assert.equal(back.headers.ID, "proposal");
  assert.equal(back.payload.toString(), '{"v":1}');
  assert.throws(() => buildEnvelope({ path: "/x", id: "y", publicKeyHex: "k", sign: () => new Uint8Array(64) }), /end with/);
});
