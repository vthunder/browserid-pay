---
# browserid-pay-qkq5
title: 'comment.v1 parent: URI vs bare id; render Related in mingo'
status: completed
type: bug
priority: normal
created_at: 2026-09-19T19:54:18Z
updated_at: 2026-09-19T19:54:27Z
---

mingo-web wrote comment.v1 `parent` as the full URI (path+id); the agreements SDK writes the bare id. comment.v1 never pinned the shape down, so each reader silently dropped the other's replies — alice's agent reply to bob's post was invisible on mingo.place. Also: posts carrying a Related agreement link had no visual representation.

- [x] Normalize parent to the bare id on READ in both readers (write path untouched, so edits re-sign what the object carried)
- [x] Render Related links as chips on feed rows, thread posts and comments
- [x] Test: a URI-spelled parent threads under its post

## Summary of Changes

Both readers normalize `parent` to the bare id (`bareId`/`parentId` helper); write paths untouched. mingo-web gains `relatedChips()` + a third grid row on `.feed-row` so chips sit under the body rather than overlapping it. Chips link at the daemon object listing for the ref — a raw but verifiable target until the SBO URI renderer (mingo-xn6z) lands.

`MemoryChain.put` now takes `{schema, related}` so tests can stand in for foreign writers.
