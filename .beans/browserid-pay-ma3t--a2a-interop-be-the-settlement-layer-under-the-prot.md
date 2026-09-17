---
# browserid-pay-ma3t
title: 'A2A interop: be the settlement layer under the protocol agents already use'
status: draft
type: feature
priority: high
created_at: 2026-09-17T14:51:03Z
updated_at: 2026-09-17T14:51:03Z
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
