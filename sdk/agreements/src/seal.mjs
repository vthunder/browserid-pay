// Sealed terms for the adjudicator — "sealed-x25519-v1" (docs/specs/agreement-v1.md):
//   epk = ephemeral X25519 public key (32 bytes)
//   shared = X25519(esk, rpk)
//   key = HKDF-SHA256(ikm = shared, salt = epk || rpk, info = "agreement.v1 seal", 32)
//   ct  = ChaCha20-Poly1305(key, nonce = 12 zero bytes, aad = epk, plaintext) || tag(16)
//   sealed = base64url(epk || ct)
// The key is single-use (fresh epk per seal), so the zero nonce is safe. Anonymous
// sender; recipient = the adjudicator's published `sealing_key` (base64url X25519).
import {
  createCipheriv, createDecipheriv, createPrivateKey, createPublicKey, diffieHellman,
  generateKeyPairSync, hkdfSync,
} from "node:crypto";

const X_PKCS8_PREFIX = Buffer.from("302e020100300506032b656e04220420", "hex");
const X_SPKI_PREFIX = Buffer.from("302a300506032b656e032100", "hex");
const INFO = Buffer.from("agreement.v1 seal");
const b64u = (b) => Buffer.from(b).toString("base64url");

export function generateSealingKey() {
  const { publicKey, privateKey } = generateKeyPairSync("x25519");
  return {
    publicKey: b64u(publicKey.export({ format: "der", type: "spki" }).subarray(-32)),
    secretKey: b64u(privateKey.export({ format: "der", type: "pkcs8" }).subarray(-32)),
  };
}

const pubObj = (raw) => createPublicKey({ key: Buffer.concat([X_SPKI_PREFIX, raw]), format: "der", type: "spki" });
const privObj = (raw) => createPrivateKey({ key: Buffer.concat([X_PKCS8_PREFIX, raw]), format: "der", type: "pkcs8" });

function derive(shared, epk, rpk) {
  return Buffer.from(hkdfSync("sha256", shared, Buffer.concat([epk, rpk]), INFO, 32));
}

/** Seal `plaintext` (string | Buffer) to `recipientPublicKeyB64`. */
export function seal(plaintext, recipientPublicKeyB64) {
  const rpk = Buffer.from(recipientPublicKeyB64, "base64url");
  if (rpk.length !== 32) throw new Error("sealing key must be 32 bytes");
  const eph = generateKeyPairSync("x25519");
  const epk = eph.publicKey.export({ format: "der", type: "spki" }).subarray(-32);
  const shared = diffieHellman({ privateKey: eph.privateKey, publicKey: pubObj(rpk) });
  const key = derive(shared, epk, rpk);
  const c = createCipheriv("chacha20-poly1305", key, Buffer.alloc(12), { authTagLength: 16 });
  c.setAAD(epk);
  const ct = Buffer.concat([c.update(Buffer.from(plaintext)), c.final(), c.getAuthTag()]);
  return b64u(Buffer.concat([epk, ct]));
}

/** Open a sealed box with the recipient's secret key. Returns a Buffer. */
export function open(sealedB64u, recipientSecretKeyB64) {
  const blob = Buffer.from(sealedB64u, "base64url");
  if (blob.length < 32 + 16) throw new Error("sealed box too short");
  const epk = blob.subarray(0, 32);
  const body = blob.subarray(32, blob.length - 16);
  const tag = blob.subarray(blob.length - 16);
  const rsk = Buffer.from(recipientSecretKeyB64, "base64url");
  const priv = privObj(rsk);
  const rpk = createPublicKey(priv).export({ format: "der", type: "spki" }).subarray(-32);
  const shared = diffieHellman({ privateKey: priv, publicKey: pubObj(epk) });
  const key = derive(shared, epk, rpk);
  const d = createDecipheriv("chacha20-poly1305", key, Buffer.alloc(12), { authTagLength: 16 });
  d.setAAD(epk);
  d.setAuthTag(tag);
  return Buffer.concat([d.update(body), d.final()]);
}
