# @browserid-ng/agreements

Agreements between agents on the browserid-pay ledger: a contract is a
**proposal** and an **acceptance** on chain, each carrying a presentation that
proves the agent was *authorized by its human to agree to this much*
(`contract:agreement` warrant with a `cap`), money sits in **escrow at a
custodian**, and a named **adjudicator**'s ruling settles disputes. Terms stay
private (a commitment on chain; the full terms sealed to the adjudicator).
Wire shapes: [`docs/specs/agreement-v1.md`](../../docs/specs/agreement-v1.md).

Node 18+, no runtime deps beyond `@browserid-ng/agent` and `@browserid-ng/verify`.

## The five verbs

```js
import { DeviceAgent } from "@browserid-ng/agent";
import { Agreements } from "@browserid-ng/agreements";

const agent = new DeviceAgent(credential);            // from @browserid-ng/wallet / requestProvision
const me = new Agreements({ agent, onApproveUrl: (u) => console.log("approve:", u) });

// 1. propose — I (alice) pay bob 50 USD for summaries. Posts the proposal;
//    returns the FULL terms (with salt) to hand to bob off-chain.
const p = await me.propose(
  { title: "Summarize 10 papers",
    obligations: [{ by: "bob@y.test", what: "deliver summaries", deadline: 1760000000 }],
    price: { amount: "50.00", currency: "USD" },
    evidence: "delivery commitment + summaries sent off-chain" },
  { acceptor: "bob@y.test" });                      // payer defaults to me; pass payer: "bob@y.test" for "I do Y if you pay X"
send(bob, p.terms);

// 2. accept — bob verifies alice's bundle + cap and the commitment, posts acceptance.
await bob.accept(p.id, termsFromAlice);

// 3. fund — the payer locks price + fees + contribution at the custodian.
await me.fund(p.id);

// 4. deliver — bob posts a delivery commitment (content goes off-chain).
await bob.deliver(p.id, { content: summaries, note: "done" });

// 5a. settle — the payer acknowledges; escrow releases to bob.
await me.settle(p.id);
// 5b. dispute — either party freezes escrow and files with the adjudicator.
await bob.dispute(p.id, { note: "never acknowledged" });

await me.status(p.id);   // { stage, proposal, acceptance, lock, evidence, ruling, escrow }
```

Warrants: the first call that needs one raises a consent request at the
broker and hands the approve URL to `onApproveUrl`. Two are asked per agent:
a ledger-write warrant (audience = the database, scopes `action:post`,
`path:/agreements/**`, `path:/<me>/**`) and, per contract, a
`contract:agreement` warrant at the **contract audience**
`sbo+raw://avail:turing:506/agreements/<id>` with
`cap = price + fees + contribution` and `counterparties = [the other party]`.

Discovery: custodian, adjudicator and the suggested contribution come from
`/providers/custodians/<identity>/listing`, `/providers/adjudicators/<identity>/listing` and `/sys/contribution`
on the ledger; the first listed is the default, and each can be overridden
in the constructor or per proposal.

## Chain writes

Objects are SBO envelopes attributed through the device-cert model: the
envelope's `Auth-Cert` is the four-object presentation for the *database*
audience, `Public-Key` is that presentation's access key (`ed25519:<hex>`),
and the signature is by the same key. Headers emitted, in canonical order:
`SBO-Version, Action, Path, ID, Type, Content-Type, Content-Length,
Content-Hash, Content-Schema, Auth-Cert, Public-Key, Signature`. Reads go
through an sbo-daemon (`/v1/object`, `/v1/list`, always with `repo=`);
writes go through the same daemon (`POST /v1/submit?repo=`), which holds the
repo's TurboDA credentials and refreshes issuer DNSSEC evidence ahead of the
write — agents carry no chain keys.

## Options

`chain` (default `DaemonChain`, or `MemoryChain` for tests), `repo`,
`verify(presentation, audience)` (default: the hosted verifier via
`@browserid-ng/verify`), `obtainWarrant(audience, scopes)` (default:
`requestWarrants` at `broker`), `custodian`, `adjudicator`, `contribution`,
`termsStore` (a `Map`-like; terms are never on chain in commitment mode, so
persist them), `http`.

## MCP

[`../agreements-mcp`](../agreements-mcp) wraps this in an MCP server with
tools `propose`, `accept`, `fund`, `deliver`, `settle`, `dispute`,
`agreement_status`, `discover`. Separate package so this one stays free of
the MCP SDK.

## Test

```
npm test
```
