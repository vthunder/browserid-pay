---
# browserid-pay-t6yl
title: 'Dispute response phase: the counterparty gets a window to reveal and rebut before the judge rules'
status: completed
type: feature
priority: normal
created_at: 2026-09-15T22:03:05Z
updated_at: 2026-09-15T22:17:52Z
parent: browserid-pay-h1f5
---

Live run: the judge ruled against bob because his delivery was a sealed commitment and nobody asked him to open it. Add: judge case state open → counterparty notified (SDK status shows "disputed, respond by T"; GET /api/cases?party=me), a `respond` verb that posts reveal/rebuttal evidence (content matching the delivery commitment, notes), then the judge rules after the response or the window (JUDGE_RESPONSE_WINDOW, default 24h; demo 60s). Judge prompt: a commitment opened in response counts as delivered content.

## Summary of Changes

Judge: dispute opens a case (state open, respond_by = now + JUDGE_RESPONSE_WINDOW; 86400 default, 60 in dev); POST /api/cases/<id>/respond (counterparty only; reveals verified against delivery commitments, matched/mismatched recorded; case → responded → ruling); GET /api/cases?party=; a 15s sweep rules open cases whose window passed; ruling prompt treats opened commitments as delivered content. SDK: deliver keeps content locally; respond(id, {note, reveals | reveal:true}) posts a response evidence object and files with the judge; status carries dispute {respond_by, you_must_respond}. MCP: respond tool. Demo: bob responds and the judge rules after. Spec: response kind + 'Dispute response phase'.
