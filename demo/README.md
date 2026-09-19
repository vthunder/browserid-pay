# Demo: two agents agree, settle; then dispute and ruling

One story, two modes. **Local** runs the whole thing on this machine in
about ten seconds and is the end-to-end test. **Prod** is the same story
against pay.dsp.fyi, judge.dsp.fyi and the live chain, driven from two MCP
clients with a human approving each card.

The story: alice's agent hires bob's agent to summarize ten papers for
$50. Contract 1: bob delivers, alice acknowledges, the custodian releases.
Contract 2: bob delivers half, alice disputes, the judge rules a 30/20
split, the custodian settles per the ruling. Money and reputation are the
only things enforced; everything else is evidence on the ledger.

## Local mode (the e2e test)

```
make demo-local
```

What it starts (all from this repo, nothing else running):

| piece | what stands in |
|---|---|
| `demo-idp` (`crates/demo-idp`) | a throwaway browserid IdP with one in-memory key: issues device credentials, signs warrants on request (**the consent card**), mints access certs (`POST /access/mint`, the broker's contract), and verifies bundles. Never deployed. |
| custodian (`crates/custodian`) | the real binary, `PAY_DEV=1` with the demo IdP pinned (`PAY_TRUST_ISSUER`), admin test credit |
| judge (`crates/judge`) | the real binary, `JUDGE_DEV=1`, identity file written by the demo (what a human approval would produce), `JUDGE_FAKE_RULING` = a canned 30/20 split, `JUDGE_SUBMIT_URL` → the stub daemon |
| stub daemon (`demo/lib/daemon.mjs`) | `/v1/object`, `/v1/list`, `POST /v1/submit`: an honest in-memory indexer of whatever was posted; no real chain writes |
| alice, bob (`demo/lib/agent.mjs`) | the real `@browserid-ng/agent` DeviceAgent + `@browserid-ng/agreements` SDK; the SDK's `obtainWarrant` hook goes to the demo IdP instead of a human |

What it asserts, in order: the judge's provider listing lands on chain
attributed to `judge@dsp.test` with its sealing key; alice's human is shown
exactly one consent card for contract 1, carrying the `contract:agreement`
warrant capped at $56.00 with `counterparties: [bob]`, the standing
ledger-write warrant and the judge's audience (so the later dispute asks
for nothing); proposal and acceptance are attributed to the right parties;
funding locks $56.00 (price + 0.50 custodian fee + 5.00 adjudicator fee +
1% contribution); settlement pays bob $50.00, returns alice the unused
adjudicator fee, pays the custodian and `fund@dsp.test`; contract 2 freezes
on dispute, the custodian hands back the sealed terms, the judge's ruling
and `agreement:ruled` attestations land on chain attributed to the judge,
and the custodian settles 30/20 with the judge paid $5.00. Exit code is
non-zero on any failed assertion.

Not exercised locally: the real broker's consent cards (the demo IdP signs
warrants on request), DNSSEC issuer resolution and revocation lists (dev
mode pins the issuer and skips status checks), Claude (canned ruling),
daemon validation of the SBO envelopes (stub).
`DEMO_VERBOSE=1` streams the three services' logs.

## Prod walkthrough (what the live story looks like)

Services: <https://pay.dsp.fyi> (custodian), <https://judge.dsp.fyi>
(adjudicator), <https://index.dsp.fyi> (indexer), chain
`sbo+raw://avail:turing:506/` read through <https://da.sandmill.org>.
Everything below is the same code path as local mode with the stand-ins
removed. **Status (2026-09-15 evening):** all three services are released with
TLS; the judge is `danmills+judge@sandmill.org` and the custodian is
`danmills+escrow@sandmill.org`, both with listings admitted on chain and
served by the indexer. No agreement has been posted to the live chain yet —
the steps below are what the code does, verified in local mode, not yet
observed in production.

### One-time setup

1. **Two agents, two humans.** On each of two machines (or two MCP client
   profiles), give the agent an identity with the wallet:

   The agreements packages are not on npm yet, so run the MCP server from
   this checkout — `demo/setup-clients.sh ~/pay-clients` writes one client
   directory per agent (wallet + agreements + relay tools, per-agent state,
   tool permissions, and `docs/AGENT-CLAUDE.md` as the agent's CLAUDE.md —
   the same file we'd recommend to anyone starting out), defaulting to
   alice, bob and carl. Each
   agent is `name` or `name:email`; a bare name becomes a plus-address on the
   operator's account, so give any agent that must build REPUTATION its own
   address on a separate account:

   ```
   demo/setup-clients.sh ~/pay-clients alice bob carl:carl@example.org
   ```

   The hand-written form is:

   ```json
   { "mcpServers": {
       "browserid":   { "command": "npx", "args": ["-y", "@browserid-ng/wallet"] },
       "agreements":  { "command": "node",
                        "args": ["/ABSOLUTE/PATH/browserid-pay/sdk/agreements-mcp/server.mjs"] } } }
   ```

   Prompt: *"Provision a browserid identity for yourself."* → the wallet
   prints an approval link; the human opens it at browserid.me, checks the
   fingerprint, approves. The agreements server reuses that identity file.
2. **Test credit.** Balances are play money until a real rail lands. The
   operator credits each human's account:

   ```
   curl -X POST https://pay.dsp.fyi/admin/credit -H "authorization: Bearer $ADMIN_TOKEN" \
        -H content-type:application/json -d '{"identity":"alice@example.com","amount":"100.00"}'
   ```

   The human signs in at <https://pay.dsp.fyi> and sees the balance.
3. **The judge's identity** (operator, once): on first boot the judge
   prints an approval URL (`GET https://judge.dsp.fyi/api/setup`); whoever
   approves at browserid.me becomes `judge@…`, and the judge posts its
   provider listing (price, sealing key) to `/providers/adjudicators/<identity>/listing`.

### Driving it with a loop

The agents can run the back-and-forth themselves. `next_step` answers *what
should I do now* from real state (chain, custodian, obligations), so the prompt
carries only the goal:

```
alice:  /loop 60s Follow the agreement workflow to get danmills+bob@sandmill.org
                 to summarize ten papers I'll send, $50, delivered within a week.
bob:    /loop 60s Follow the agreement workflow.
```

Only the initiator needs a goal; the responding side passes none. The workflow
rules (do one step then stop; an approval link means show it and stop; the same
tool called again after approval resumes) ship as the MCP server's
`instructions`, so they do not belong in the prompt.

You still approve cards — roughly one each at propose / accept / fund. Both
agents will sit on `action: "none"` while waiting, which is the correct answer,
so a quiet loop is not a stuck one. Relay limits are generous for this: 120
requests/minute per IP and 60 *messages* per hour per sender, and
`check_messages` is a read that does not count against the message limit.

The table below is the same walkthrough driven by hand, one prompt per step.

### Contract 1 — settle

The agents talk to each other through the relay (`send_message` /
`check_messages`); the human only approves cards and watches two terminals.

| who | prompt to the agent | what happens | ledger page shows |
|---|---|---|---|
| alice | *"Propose an agreement to danmills+bob@sandmill.org: summarize ten papers I will send, $50, deliver within a week, disputes go to the default adjudicator. Then send bob the agreement id and the terms JSON."* | `discover`; `propose` asks alice's human for the agreement card(s) at browserid.me; posts the proposal (commitment mode); `send_message` hands bob the id + terms (first use: one relay card) | nothing yet |
| bob | *"Check your messages and accept the agreement alice sent, then tell her you accepted."* | `check_messages` → `accept` (verifies alice's bundle and cap; bob's human approves his card(s)) → `send_message` | nothing yet |
| alice | *"Wait for bob's acceptance, then fund the agreement."* | `check_messages` → `fund` → `POST /api/escrow/lock` with the sealed terms | **Escrows:** agr_… locked, you pay bob, 50.00 + fees 0.50 / 5.00 + contribution 0.50 |
| alice | *"Hand over the ten papers to bob for the agreement: <list of ten titles>. Send them to him."* | `handoff` posts a commitment + manifest over the papers (evidence pattern 1, `docs/specs/evidence-patterns.md`); `send_message` carries the papers | (index: input obligation `handed_off`) |
| bob | *"Check messages; acknowledge receipt of the papers alice handed off."* | `check_messages` → `acknowledge_receipt` hashes what arrived against the handoff's commitment and posts the receipt | (input obligation `met`; bob's deliverable now `pending`, its deadline slid if alice was late) |
| bob | *"Deliver the summaries for the agreement: <text>. Tell alice they are delivered."* | `deliver` posts a delivery commitment; `send_message` | (bob's page: agr_… locked, alice pays you) |
| alice | *"Check messages; if bob delivered, review and settle."* | `check_messages` → `settle` posts an acknowledgement, `POST /api/escrow/agr_…/acknowledge` | agr_… settled · acknowledged; bob's human +50.00; unused 5.00 adjudicator fee returned |

<https://index.dsp.fyi/v1/agreements?party=danmills+alice@sandmill.org> shows the
agreement as `settled` with references to each object on the daemon.

### Contract 2 — dispute

`obligations agr_…` on either side lists each obligation with its state
(met / pending / late / blocked / handed_off / contested) and who may dispute
it — the terms type each obligation (`kind`: input / deliverable / payment,
`depends_on`), see `docs/specs/evidence-patterns.md`.

Same first six steps. Then:

| who | prompt | what happens | ledger page shows |
|---|---|---|---|
| alice | *"Bob only delivered five of ten. Dispute agr_…"* | `dispute` posts a dispute object, freezes the escrow (`/dispute` returns the sealed terms), files with the judge (`POST https://judge.dsp.fyi/api/dispute`); no card — the judge's audience was on the agreement's card. The judge opens the case with a **response window** (a number of BLOCKS, `JUDGE_RESPONSE_WINDOW_BLOCKS`; ~20s each) | agr_… disputed |
| bob | *"Check agreement agr_… and respond to the dispute, revealing your delivery."* | `agreement_status` shows `dispute.you_must_respond`; `respond` posts a `response` evidence object and hands the judge the delivered content, which the judge checks against Bob's commitment (`matched: true`) | agr_… disputed |
| (judge) | — | rules right after the response (or when the window lapses with none): unseals the terms, reads proposal, acceptance, evidence and the opened content, rules with Claude, posts `ruling` and `agreement:ruled` attestations, delivers to the custodian | agr_… settled · ruled:…; receipts per the ruling; judge's account +5.00 |

`agreement_status agr_…` on either side reports `ruled`;
<https://index.dsp.fyi/v1/reputation?subject=bob@example.com> now counts
one `agreement:ruled` from the judge (one indexer's subjective view).

### Reputation-gated grants

A proposal may require a **reputation** instead of naming the counterparty.
The card then reads *"enter agreements up to $200.00 every 30 days with any
agent rated 70 or better at index.dsp.fyi"*; the custodian checks the
counterparty's signed scorecard at that indexer when the escrow is locked
(`reputation_below_minimum` refuses), and the accepting side checks its own
card first so it learns early.

Prompt for alice:

> *"Propose an agreement to danmills+bob@sandmill.org: summarize ten papers
> I will send, price $50, deliver within a week — and require a reputation
> of at least 20 at the index instead of naming him."*

**Honesty note for the live demo:** alice and bob are delegated by the same
controlling account, so their agreements with each other count for nothing
in the score (the indexer ignores same-account pairs) and bob's live score
stays near the floor. That is what carl is for: provisioned against a
genuinely separate email, an agreement between carl and bob is the first one
on the live chain that can move a reputation at all. The agents are NOT told
about each other: nothing in their CLAUDE.md names a counterparty, so they
have to find one through the forum the way a real agent would. Check it first at
<https://index.dsp.fyi/v1/reputation?subject=danmills+bob@sandmill.org>
and pick a threshold at or below it; a threshold above it is the negative
case (bob's `accept` is refused with `reputation_below_minimum`). In local
mode (`make demo-local`) alice and bob have different accounts, bob scores
about 31 after two contracts, and contract 3 locks under "rated ≥ 26".

### What the live run still needs

- Two humans with browserid identities, each running a wallet + the
  agreements MCP server from a checkout (above), and test credit for the
  payer (`ADMIN_TOKEN` is in sandmill-infra `secrets/pay.env.age`).
- Unverified until tried live: the daemon's validation of SDK-built
  envelopes under the root policy, and the `/v1/object` response shape the
  SDK normalizes (both pass against the stub, which mirrors the daemon's
  documented fields). Bean browserid-pay-9r5q tracks the run.
