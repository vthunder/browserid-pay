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
`sbo+raw://avail:turing:530/` read through <https://da.sandmill.org>.
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
   this checkout (`npm install` once in `sdk/agreements-mcp`):

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

### Contract 1 — settle

| who | prompt to the agent | what happens | ledger page shows |
|---|---|---|---|
| alice | *"Propose an agreement to bob@example.com: summarize ten papers I will send, $50, deliver within a week, dispute goes to the default adjudicator. Send bob the terms."* | `discover` picks pay.dsp.fyi, judge, 1% contribution; `propose` asks alice's human for **one card** at browserid.me listing every grant this agreement needs: *enter agreements up to $56.00 each with bob@example.com*, *record agreements on the browserid-pay ledger* (standing, first agreement only) and *dispute at judge.dsp.fyi*; posts the proposal (commitment mode) to the chain | nothing yet |
| bob | *"Accept agreement agr_… with these terms: …"* | `accept` verifies alice's bundle and cap, asks bob's human for his one card (same three grants), posts the acceptance | nothing yet |
| alice | *"Fund agreement agr_…"* | `fund` → `POST /api/escrow/lock` with the sealed terms | **Escrows:** agr_… locked, you pay bob, 50.00 + fees 0.50 / 5.00 + contribution 0.50; balance down 56.00 |
| bob | *"Deliver the summaries for agr_…: <text>"* | `deliver` posts a delivery commitment | (bob's page: agr_… locked, alice pays you) |
| alice | *"The summaries are fine — settle agr_…"* | `settle` posts an acknowledgement, `POST /api/escrow/agr_…/acknowledge` | agr_… settled · acknowledged; receipts: −50.00 to bob, +5.00 adjudicator fee returned; bob's balance +50.00 |

<https://index.dsp.fyi/v1/agreements?party=alice@example.com> shows the
agreement as `settled` with references to each object on the daemon.

### Contract 2 — dispute

Same first four steps. Then:

| who | prompt | what happens | ledger page shows |
|---|---|---|---|
| alice | *"Bob only delivered five of ten. Dispute agr_…"* | `dispute` posts a dispute object, freezes the escrow (`/dispute` returns the sealed terms), files with the judge (`POST https://judge.dsp.fyi/api/dispute`); no card — the judge's audience was on the agreement's card | agr_… disputed |
| (judge) | — | unseals the terms, reads proposal, acceptance and evidence from the chain, rules with Claude, posts `ruling` and `agreement:ruled` attestations, delivers to the custodian | agr_… settled · ruled:split; receipts per the split; judge's account +5.00 |

`agreement_status agr_…` on either side reports `ruled`;
<https://index.dsp.fyi/v1/reputation?subject=bob@example.com> now counts
one `agreement:ruled` from the judge (one indexer's subjective view).

### What the live run still needs

- Two humans with browserid identities, each running a wallet + the
  agreements MCP server from a checkout (above), and test credit for the
  payer (`ADMIN_TOKEN` is in sandmill-infra `secrets/pay.env.age`).
- Unverified until tried live: the daemon's validation of SDK-built
  envelopes under the root policy, and the `/v1/object` response shape the
  SDK normalizes (both pass against the stub, which mirrors the daemon's
  documented fields). Bean browserid-pay-9r5q tracks the run.
