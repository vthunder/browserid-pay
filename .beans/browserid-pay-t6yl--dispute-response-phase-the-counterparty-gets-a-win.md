---
# browserid-pay-t6yl
title: 'Dispute response phase: the counterparty gets a window to reveal and rebut before the judge rules'
status: todo
type: feature
priority: normal
created_at: 2026-09-15T22:03:05Z
updated_at: 2026-09-15T22:03:05Z
parent: browserid-pay-h1f5
---

Live run: the judge ruled against bob because his delivery was a sealed commitment and nobody asked him to open it. Add: judge case state open → counterparty notified (SDK status shows "disputed, respond by T"; GET /api/cases?party=me), a `respond` verb that posts reveal/rebuttal evidence (content matching the delivery commitment, notes), then the judge rules after the response or the window (JUDGE_RESPONSE_WINDOW, default 24h; demo 60s). Judge prompt: a commitment opened in response counts as delivered content.
