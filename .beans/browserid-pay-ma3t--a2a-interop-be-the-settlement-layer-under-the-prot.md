---
# browserid-pay-ma3t
title: 'A2A interop: be the settlement layer under the protocol agents already use'
status: draft
type: feature
priority: high
created_at: 2026-09-17T14:51:03Z
updated_at: 2026-09-18T18:04:37Z
---

A2A (Google → Linux Foundation, 2026) is where agent interop adoption is. It is a transport and task-orchestration protocol, and its gaps are exactly what this project already has — so the play is to co-opt it rather than compete with it: let A2A do discovery and task plumbing, and be the identity, money and adjudication layer underneath.

## What A2A has

- **Agent Card** at a well-known URI (`/.well-known/agent-card.json`), or via curated registries queryable by skill/tag, or direct config. Carries identity, endpoint, capabilities, auth schemes, and `AgentSkill` objects (id, name, description, inputModes, outputModes, examples).
- **Task lifecycle**: `SUBMITTED → WORKING → COMPLETED | FAILED | CANCELED | REJECTED`, plus interrupted states `INPUT_REQUIRED` and `AUTH_REQUIRED`.
- **Methods**: SendMessage, SendStreamingMessage, GetTask, ListTasks, CancelTask, SubscribeToTask, push-notification config, GetExtendedAgentCard.
- **Task object**: id, contextId, status, artifacts, history, metadata.

## What A2A does NOT have (verified against the spec, 2026-09-17)

- **No payment, escrow, contracts or binding terms.** None. The spec is purely communication and orchestration.
- **No peer identity establishment or trust negotiation** beyond conventional web auth (OAuth2, API keys, mTLS). An Agent Card asserts capability with no evidence behind it.
- **No reputation.** The community proposal (discussion #1631, NOT adopted) opens with "I found 200 agents that claim to do translation. Which one should I actually trust?" and argues for capability-specific, transaction-bound ratings — which is what our attestations already are, by construction.
- **No way to express a NEED.** Discovery is provider-advertises-then-client-queries only. See the RFQ bean.

## The mapping to explore

| A2A | here |
|---|---|
| Agent Card | `/providers/agents/<identity>/listing` — same shape as the existing custodian/adjudicator listings, signed and on chain |
| auth scheme | a browserid bundle as an A2A security scheme: real peer identity, not a bearer token |
| Task | an agreement — but A2A tasks are not binding and carry no money |
| `SUBMITTED` / `REJECTED` | proposal / declined |
| `WORKING` | funded + escrow locked |
| `COMPLETED` | delivery + settle |
| *(nothing)* | escrow, evidence, dispute, ruling, attestations |

## Questions

- [ ] Serve an Agent Card for an agent using this stack — what goes in `skills`, and does it reference the on-chain listing and reputation?
- [ ] Can a browserid presentation be registered as an A2A security scheme, or does it have to ride in metadata?
- [ ] Does an A2A task wrap an agreement, or does an agreement reference a task id? (`contextId` and `metadata` are the hooks.)
- [ ] Is there an A2A extension mechanism to carry terms/escrow refs, or does that stay out of band?
- [ ] Distribution: is the win "agents on A2A can hire a browserid-pay agent", or "any A2A pair can settle through us"? The second is bigger and needs no one to adopt our client.

Raised 2026-09-17 after surveying the field. Independent of the RFQ bean — this buys reach, that adds a primitive nobody has.

## Survey against A2A 1.0 (2026-09-18)

Re-verified against the live spec. **A2A 1.0 shipped 2026-03-12** and moved on from what this bean recorded in two ways that matter, and one that cuts against an assumption elsewhere.

### 1. Agent Cards are now SIGNED — and we already speak the format

`AgentCardSignature` (spec §4.4.7, §8.4): a **JWS (RFC 7515) over the JCS (RFC 8785) canonical form** of the card. Protected header MUST carry `alg`, `typ: "JOSE"` and `kid`, MAY carry `jku`; clients SHOULD verify at least one signature.

The bean said a card "asserts capability with no evidence behind it". That is still true of the **capability claim** — nothing backs "I do translation" — but the card's **origin** is now verifiable, which it was not before.

**And JCS is already ours.** `sdk/agreements/src/jcs.mjs` canonicalizes with JCS for `commitment()`. Serving a signed Agent Card is not new machinery for us; it is the same canonicalization we already sign agreements with.

### 2. There IS an extension mechanism — this answers the open question

`AgentExtension` on the card: `{ uri, description, required, params }`. A client activates one by sending an **`A2A-Extensions`** HTTP header listing extension URIs; the agent echoes back the ones it activated. Data rides in the **`metadata` map on existing structures, keyed by the extension URI** — extensions may not add fields to core types.

Four kinds are sanctioned, and two are exactly what we need:
- **data-only** — structured info on the card
- **profile-based** — overlay requirements on existing messages via metadata
- **method extensions** — new RPC methods ("Extended Skills")
- **state machine** — **additional task states or transitions**

So "does an A2A task wrap an agreement, or does an agreement reference a task id" has a third answer: **an extension declares the escrowed-agreement profile**, carries the agreement id / custodian / terms commitment in `metadata`, and may add the states A2A lacks (funded, locked, disputed, ruled). `required: false` means an agent that does not understand it still interoperates — which is what makes the ambitious version viable.

### 3. browserid CANNOT be a native security scheme

`securitySchemes` is a closed OpenAPI-style set: APIKey, HTTPAuth, OAuth2, OpenIdConnect, MutualTls. A browserid presentation does **not** fit. It rides as an **extension**, not a scheme. That settles that question: stop looking for a scheme slot.

### 4. The correction that matters most: THERE IS NO STANDARD REGISTRY

> "the current A2A specification does not prescribe a standard API for curated registries"

Discovery is: a well-known URI per domain (`/.well-known/agent-card.json`), *unstandardised* curated registries, or direct config. **The card FORMAT is standard; the DIRECTORY is not.** There is no single place the world queries.

This undercuts a premise used elsewhere — `docs/plans/2026-09-18-agent-forum-design.md` §2b and `browserid-pay-h4mu` both lean on "Agent Cards are the directory format everyone already queries". They are not a directory at all. To find an agent by well-known URI you must already know its domain.

Consequences:
- **Serving a card is still worth it** — cheap, standard, and it is how an agent we already know about is described. But it buys *description*, not *discovery*.
- **The forum's board case gets stronger, not weaker.** If there is no queryable directory, "who is good at X" has nowhere else to be asked.
- A curated registry is an unoccupied slot. Not obviously ours to fill, and it would compete rather than co-opt.

### 5. Unchanged and confirmed

No payment, escrow, pricing or terms anywhere in the spec. No reputation. No way to express a need. The gap is still exactly the shape of this project.

Also new since the bean: an `interfaces` array (`AgentInterface`, endpoint + protocol) and an `extendedAgentCard` capability.

### Where this leaves the strategic question

The bean asked: *"agents on A2A can hire a browserid-pay agent"* or *"any A2A pair can settle through us"*. The extension mechanism makes the second **technically available** — an escrowed-agreement extension is additive, optional, and needs nobody to adopt our client. That is the bigger prize and it is not blocked by the protocol.

What is not solved is reach: with no registry, nobody finds the extension by browsing. Adoption would have to come from somewhere else — which is the same distribution problem the forum epic exists to attack.

- [x] can a browserid presentation be an A2A security scheme — **no**, closed set; it is an extension
- [x] is there an extension mechanism for terms/escrow refs — **yes**, `AgentExtension` + `A2A-Extensions` header + URI-keyed `metadata`, and it can add task states
- [x] does a task wrap an agreement or vice versa — **neither**: an extension profiles the task and carries the agreement reference in metadata
- [ ] draft the escrowed-agreement extension URI and its metadata shape
- [ ] serve a signed Agent Card for a demo agent (JWS over JCS — reuse `jcs.mjs`), with `skills` and a reference to the on-chain listing + scorecard
- [ ] decide whether the card references reputation by URL (indexer scorecard) or embeds a snapshot
