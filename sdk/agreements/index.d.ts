export type ScopeEntry = string | { scope: string; mode?: "auto" | "prompt"; cap?: { amount: string; currency: string; window?: string }; counterparties?: string[]; max_duration?: string };

export interface Terms {
  title?: string; description?: string;
  obligations: { by: string; what: string; deadline?: number }[];
  price: { amount: string; currency: string };
  fees?: { custodian?: string; adjudicator?: string };
  evidence?: string;
}
export interface FullTerms extends Terms { v: 1; id: string; salt: string; fees: { custodian: string; adjudicator: string }; contribution: { recipient: string | null; percentage: number } }

export interface Chain {
  get(path: string, id: string): Promise<{ path: string; id: string; payload: any; author: string | null; block: number | null } | null>;
  list(prefix: string): Promise<{ path: string; id: string; payload: any; author: string | null; block: number | null }[]>;
  submit(wire: Buffer): Promise<string>;
}

export interface AgreementsOptions {
  agent: any; // DeviceAgent from @browserid-ng/agent
  chain?: Chain; repo?: string;
  verify?: (presentation: string, audience: string) => Promise<{ ok: boolean; email?: string; grantee?: string; scopes?: ScopeEntry[]; reason?: string }>;
  obtainWarrant?: (audience: string, scopes: ScopeEntry[]) => Promise<void>;
  onApproveUrl?: (url: string) => void;
  broker?: string;
  custodian?: { origin: string; price?: { amount: string; currency: string } };
  adjudicator?: { identity: string; origin?: string; sealing_key?: string; price?: { amount: string; currency: string } };
  contribution?: { recipient: string; percentage: number };
  http?: typeof fetch;
}

export class AgreementError extends Error { code: string }

export class Agreements {
  constructor(o: AgreementsOptions);
  readonly email: string;
  discover(): Promise<{ custodians: any[]; adjudicators: any[]; contribution: { recipient: string; percentage: number } | null }>;
  propose(terms: Terms, o: { acceptor: string; payer?: string; adjudicator?: any; custodian?: any; contribution?: any; expires?: number; mode?: "commitment" | "public" }): Promise<{ id: string; terms: FullTerms; salt: string; commitment: string; proposal: any; sealed_terms: string | null; submission: string }>;
  accept(id: string, terms?: FullTerms): Promise<{ id: string; acceptance: any; submission: string }>;
  fund(id: string): Promise<any>;
  deliver(id: string, o?: { content?: any; commitment?: string; note?: string; public?: boolean }): Promise<any>;
  settle(id: string, o?: { note?: string }): Promise<any>;
  dispute(id: string, o?: { note?: string }): Promise<any>;
  status(id: string): Promise<{ id: string; stage: string; proposal: any; acceptance: any; lock: any; ruling: any; evidence: any[]; escrow: any }>;
  post(path: string, id: string, payload: any, schema?: string): Promise<{ submission: string; path: string; id: string }>;
}

export function contractAudience(id: string, repo?: string): string;
export function agreementPath(id: string): string;
export function totalOf(terms: FullTerms | Terms): number;
export class DaemonChain implements Chain { constructor(o?: { daemon?: string; repo?: string; turbo?: { endpoint?: string; appId?: number; key?: string }; http?: typeof fetch }); get: Chain["get"]; list: Chain["list"]; submit: Chain["submit"]; repo: string }
export class MemoryChain implements Chain { get: Chain["get"]; list: Chain["list"]; submit: Chain["submit"]; put(path: string, id: string, payload: any, author?: string): void; objects: any[] }
export const CHAIN_DEFAULTS: { daemon: string; repo: string; turbo: { endpoint: string; appId: number } };
export function buildEnvelope(m: any): { wire: Buffer; headers: Record<string, string> };
export function parseEnvelope(wire: Buffer | Uint8Array): { headers: Record<string, string>; payload: Buffer };
export function pubkeyHex(raw32: Uint8Array): string;
export function canonicalize(v: any): string;
export function commitment(v: any): string;
export function seal(plaintext: string | Buffer, recipientPublicKeyB64: string): string;
export function open(sealed: string, recipientSecretKeyB64: string): Buffer;
export function generateSealingKey(): { publicKey: string; secretKey: string };
