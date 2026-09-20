//! The reverse index. SQLite; rebuilt from chain reads on every poll (the
//! chain is small and the daemon's list reads are the source of truth).
//!
//! Identity matching is **exact string** (domain part lowercased): the
//! daemon exposes no subject→controller resolution, so `alice` and
//! `alice@x.test` stay distinct here. When the daemon resolves names this is
//! the one place to widen.

use crate::chain::{Obj, StateRoot};
use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct Index(Arc<Mutex<Connection>>);

/// A reference a client can re-fetch from the daemon and verify itself.
#[derive(Debug, Clone, Serialize)]
pub struct Reference {
    pub path: String,
    pub id: String,
    pub block: u64,
    pub object_hash: String,
}

impl From<&Obj> for Reference {
    fn from(o: &Obj) -> Self {
        Self { path: o.path.clone(), id: o.id.clone(), block: o.block, object_hash: o.object_hash.clone() }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Agreement {
    pub id: String,
    pub state: String,
    pub proposer: String,
    pub acceptor: String,
    pub payer: String,
    pub payee: String,
    pub adjudicator: String,
    pub custodian: String,
    pub mode: String,
    /// The proposal's offer expiry, as a BLOCK HEIGHT.
    pub expires_block: Option<i64>,
    pub proposal_block: u64,
    pub acceptance_block: Option<u64>,
    pub lock_block: Option<u64>,
    pub ruling_block: Option<u64>,
    /// The controlling account (warrant grantor) behind each party, read from
    /// the presentation `bundle` on the proposal / acceptance. NOT a person
    /// and not sybil-proof — anyone can mint accounts; it only lets an
    /// indexer refuse to count an agreement an account made with itself.
    pub proposer_grantor: Option<String>,
    pub acceptor_grantor: Option<String>,
    /// The ruling payload (outcome, split, late_party, partial) when ruled.
    pub ruling: Option<Value>,
    pub evidence: Vec<Value>,
    pub references: Vec<Reference>,
}

/// The `grantor` of the warrant inside a four-object presentation bundle
/// (`access_cert~assertion~warrant~config_cert`), read structurally — the
/// object is already on chain and attributed, so no signature check here.
pub fn grantor_of_bundle(bundle: &str) -> Option<String> {
    use base64::Engine;
    let warrant = bundle.split('~').nth(2)?;
    let payload = warrant.split('.').nth(1)?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(payload).ok()?;
    let v: Value = serde_json::from_slice(&bytes).ok()?;
    v.get("grantor").and_then(|g| g.as_str()).map(norm)
}

#[derive(Debug, Clone, Serialize)]
pub struct Attestation {
    pub issuer: String,
    pub subject: String,
    #[serde(rename = "type")]
    pub typ: String,
    pub value: Value,
    pub issued_at: Option<i64>,
    pub expires: Option<i64>,
    pub reference: Reference,
}

pub fn norm(identity: &str) -> String {
    match identity.rsplit_once('@') {
        Some((local, domain)) => format!("{local}@{}", domain.to_ascii_lowercase()),
        None => identity.to_string(),
    }
}

/// `/providers/<kind>/<identity>/` + `listing` → `(kind, identity)`; anything
/// else under `/providers/` is not a listing.
pub fn provider_ref(path: &str, id: &str) -> Option<(String, String)> {
    if id != "listing" {
        return None;
    }
    let rest = path.strip_prefix("/providers/")?.strip_suffix('/')?;
    let (kind, name) = rest.split_once('/')?;
    if (kind != "custodians" && kind != "adjudicators") || name.is_empty() || name.contains('/') {
        return None;
    }
    Some((kind.to_string(), name.to_string()))
}

impl Index {
    pub fn open(path: &str) -> Result<Self> {
        let c = Connection::open(path)?;
        c.execute_batch(
            "PRAGMA journal_mode=WAL;
             CREATE TABLE IF NOT EXISTS meta (k TEXT PRIMARY KEY, v TEXT NOT NULL);
             -- The index is a cache rebuilt from chain reads on every poll, so a
             -- schema change simply drops the derived tables (a live database
             -- that predates a new column would otherwise break the indexes).
             DROP TABLE IF EXISTS agreements;
             DROP TABLE IF EXISTS attestations;
             DROP TABLE IF EXISTS providers;
             CREATE TABLE IF NOT EXISTS agreements (
               id TEXT PRIMARY KEY, state TEXT NOT NULL,
               proposer TEXT NOT NULL, acceptor TEXT NOT NULL, payer TEXT NOT NULL, payee TEXT NOT NULL,
               adjudicator TEXT NOT NULL, custodian TEXT NOT NULL, mode TEXT NOT NULL, expires_block INTEGER,
               proposal_block INTEGER NOT NULL, acceptance_block INTEGER, lock_block INTEGER, ruling_block INTEGER,
               proposer_grantor TEXT, acceptor_grantor TEXT, ruling TEXT,
               evidence TEXT NOT NULL, refs TEXT NOT NULL
             );
             CREATE INDEX IF NOT EXISTS agreements_pg ON agreements(proposer_grantor);
             CREATE INDEX IF NOT EXISTS agreements_ag ON agreements(acceptor_grantor);
             CREATE INDEX IF NOT EXISTS agreements_proposer ON agreements(proposer);
             CREATE INDEX IF NOT EXISTS agreements_acceptor ON agreements(acceptor);
             CREATE INDEX IF NOT EXISTS agreements_adjudicator ON agreements(adjudicator);
             CREATE TABLE IF NOT EXISTS attestations (
               path TEXT NOT NULL, id TEXT NOT NULL, issuer TEXT NOT NULL, subject TEXT NOT NULL, typ TEXT NOT NULL,
               value TEXT NOT NULL, issued_at INTEGER, expires INTEGER, block INTEGER NOT NULL, object_hash TEXT NOT NULL,
               PRIMARY KEY (path, id)
             );
             CREATE INDEX IF NOT EXISTS attestations_subject ON attestations(subject, typ);
             DROP TABLE IF EXISTS refs;
             CREATE TABLE IF NOT EXISTS refs (
               from_path TEXT NOT NULL, from_id TEXT NOT NULL,
               rel TEXT NOT NULL, ref TEXT NOT NULL,
               author TEXT NOT NULL, block INTEGER NOT NULL,
               -- Whether the author is a party to the agreement it points at.
               -- An UNVERIFIED reference is a claim someone made about
               -- themselves: kept, so it can still be displayed, but never
               -- counted (browserid-pay-ctnd).
               verified INTEGER NOT NULL,
               PRIMARY KEY (from_path, from_id, rel, ref)
             );
             CREATE INDEX IF NOT EXISTS refs_ref ON refs(ref, rel);
             -- Forum posts and comments. The reverse index (refs) answers who
             -- pointed at a thing; this answers what is being said and to whom,
             -- which is what an agent needs to notice a reply to its own
             -- thread, or a need it could serve.
             DROP TABLE IF EXISTS posts;
             CREATE TABLE IF NOT EXISTS posts (
               path TEXT NOT NULL, id TEXT NOT NULL,
               community TEXT NOT NULL, space TEXT NOT NULL,
               kind TEXT NOT NULL,            -- post | comment
               author TEXT NOT NULL,
               parent TEXT,                   -- bare id of the post/comment answered
               body TEXT NOT NULL,
               block INTEGER NOT NULL, object_hash TEXT NOT NULL,
               PRIMARY KEY (path, id)
             );
             CREATE INDEX IF NOT EXISTS posts_author ON posts(author);
             CREATE INDEX IF NOT EXISTS posts_parent ON posts(parent);
             CREATE INDEX IF NOT EXISTS posts_space ON posts(space, block DESC);

             CREATE TABLE IF NOT EXISTS providers (
               kind TEXT NOT NULL, name TEXT NOT NULL, payload TEXT NOT NULL, block INTEGER NOT NULL, object_hash TEXT NOT NULL,
               PRIMARY KEY (kind, name)
             );",
        )?;
        Ok(Self(Arc::new(Mutex::new(c))))
    }

    fn with<T>(&self, f: impl FnOnce(&mut Connection) -> Result<T>) -> Result<T> {
        let mut c = self.0.lock().expect("index lock");
        f(&mut c)
    }

    /// Replace the whole index from one consistent set of chain reads.
    pub fn rebuild(&self, agreements: &[Obj], attestations: &[Obj], providers: &[Obj], contribution: Option<&Obj>, root: &StateRoot) -> Result<()> {
        self.rebuild_with_refs(agreements, attestations, providers, contribution, &[], root)
    }

    /// As [`rebuild`], plus `referencing`: every object carrying a `Related`
    /// header, from anywhere on the chain. Forum posts are the first of these,
    /// but nothing here knows what a post is — a reference is a reference.
    pub fn rebuild_with_refs(&self, agreements: &[Obj], attestations: &[Obj], providers: &[Obj], contribution: Option<&Obj>, referencing: &[Obj], root: &StateRoot) -> Result<()> {
        self.rebuild_all(agreements, attestations, providers, contribution, referencing, &[], root)
    }

    pub fn rebuild_all(&self, agreements: &[Obj], attestations: &[Obj], providers: &[Obj], contribution: Option<&Obj>, referencing: &[Obj], posts: &[Obj], root: &StateRoot) -> Result<()> {
        self.store_posts(posts)?;
        self.rebuild_core(agreements, attestations, providers, contribution, referencing, root)
    }

    /// `/communities/<c>/spaces/<s>/` → (community, space). Anything else is
    /// not a forum object and is skipped rather than guessed at.
    fn community_space(path: &str) -> Option<(String, String)> {
        let p: Vec<&str> = path.trim_matches('/').split('/').collect();
        match p.as_slice() {
            ["communities", c, "spaces", s] => Some((c.to_string(), s.to_string())),
            _ => None,
        }
    }

    fn store_posts(&self, posts: &[Obj]) -> Result<()> {
        self.with(|c| {
            let tx = c.transaction()?;
            tx.execute_batch("DELETE FROM posts;")?;
            for o in posts {
                if o.id == "_config" { continue; }
                let Some((community, space)) = Self::community_space(&o.path) else { continue };
                let kind = match o.content_schema.as_deref() {
                    Some("post.v1") => "post",
                    Some("comment.v1") => "comment",
                    _ => continue,
                };
                let v = o.value.clone().unwrap_or_default();
                let body = v.get("body").and_then(|b| b.as_str()).unwrap_or("").to_string();
                // `parent` is spelled as a bare id by the agreements SDK and as
                // a full URI by mingo's SPA; comment.v1 never pinned it down.
                // Normalise to the bare id so a reply threads either way.
                let parent = v.get("parent").and_then(|p| p.as_str())
                    .map(|p| p.rsplit('/').next().unwrap_or(p).to_string())
                    .filter(|p| !p.is_empty());
                let author = norm(o.owner_ref.as_deref().unwrap_or(&o.creator));
                tx.execute(
                    "INSERT OR REPLACE INTO posts(path,id,community,space,kind,author,parent,body,block,object_hash)
                     VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
                    params![o.path, o.id, community, space, kind, author, parent, body, o.block as i64, o.object_hash],
                )?;
            }
            tx.commit()?;
            Ok(())
        })
    }

    fn rebuild_core(&self, agreements: &[Obj], attestations: &[Obj], providers: &[Obj], contribution: Option<&Obj>, referencing: &[Obj], root: &StateRoot) -> Result<()> {
        // The head this rebuild is consistent with: `expires_block` is a block
        // height, so whether an offer has lapsed is decided by the chain, not
        // by this process's clock (browserid-pay-e1ou).
        let derived = derive_agreements(agreements, attestations, root.block);
        self.with(|c| {
            let tx = c.transaction()?;
            tx.execute_batch("DELETE FROM agreements; DELETE FROM attestations; DELETE FROM providers; DELETE FROM refs;")?;
            for a in &derived {
                tx.execute(
                    "INSERT INTO agreements(id,state,proposer,acceptor,payer,payee,adjudicator,custodian,mode,expires_block,proposal_block,acceptance_block,lock_block,ruling_block,proposer_grantor,acceptor_grantor,ruling,evidence,refs)
                     VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19)",
                    params![a.id, a.state, a.proposer, a.acceptor, a.payer, a.payee, a.adjudicator, a.custodian, a.mode, a.expires_block,
                            a.proposal_block as i64, a.acceptance_block.map(|b| b as i64), a.lock_block.map(|b| b as i64), a.ruling_block.map(|b| b as i64),
                            a.proposer_grantor, a.acceptor_grantor, a.ruling.as_ref().map(|r| r.to_string()),
                            serde_json::to_string(&a.evidence)?, serde_json::to_string(&a.references)?],
                )?;
            }
            for o in attestations {
                let Some(att) = parse_attestation(o) else { continue };
                tx.execute(
                    "INSERT OR REPLACE INTO attestations(path,id,issuer,subject,typ,value,issued_at,expires,block,object_hash) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
                    params![o.path, o.id, att.issuer, att.subject, att.typ, att.value.to_string(), att.issued_at, att.expires, o.block as i64, o.object_hash],
                )?;
            }
            // The reverse index. `verified` says whether the author was a
            // PARTY to the agreement it points at — without that, `endorses`
            // and `agreement` are claims a poster makes about itself, and
            // anyone could attach themselves to someone else's settlement.
            let parties: std::collections::HashMap<&str, (&str, &str)> = derived
                .iter()
                .map(|a| (a.id.as_str(), (a.proposer.as_str(), a.acceptor.as_str())))
                .collect();
            for o in referencing {
                for r in &o.related {
                    let verified = match agreement_id_of(&r.reference) {
                        // Points at an agreement: the author must be a party.
                        Some(id) => parties
                            .get(id)
                            .is_some_and(|(p, a)| norm(&o.creator) == norm(p) || norm(&o.creator) == norm(a)),
                        // Points at anything else (an identity, a licence, a
                        // thread): there is nothing to check it against here.
                        None => false,
                    };
                    tx.execute(
                        "INSERT OR REPLACE INTO refs(from_path,from_id,rel,ref,author,block,verified) VALUES(?1,?2,?3,?4,?5,?6,?7)",
                        params![o.path, o.id, r.rel, r.reference, norm(&o.creator), o.block as i64, verified as i64],
                    )?;
                }
            }
            for o in providers {
                // /providers/<kind>/<identity>/  +  ID `listing`  (agreement-v1.md
                // "Providers": the identity is a PATH segment because root-policy
                // grant patterns match paths, never ids). Anything else under
                // /providers/ is ignored.
                let Some((kind, name)) = provider_ref(&o.path, &o.id) else { continue };
                tx.execute(
                    "INSERT OR REPLACE INTO providers(kind,name,payload,block,object_hash) VALUES(?1,?2,?3,?4,?5)",
                    params![kind, name, o.json().to_string(), o.block as i64, o.object_hash],
                )?;
            }
            let contribution = contribution.map(|o| json!({ "value": o.json(), "reference": Reference::from(o) })).unwrap_or(Value::Null);
            tx.execute("INSERT OR REPLACE INTO meta(k,v) VALUES('contribution',?1)", params![contribution.to_string()])?;
            tx.execute("INSERT OR REPLACE INTO meta(k,v) VALUES('state_root',?1)", params![serde_json::to_string(root)?])?;
            tx.execute("INSERT OR REPLACE INTO meta(k,v) VALUES('indexed_at',?1)", params![chrono::Utc::now().timestamp().to_string()])?;
            tx.commit()?;
            Ok(())
        })
    }

    pub fn meta(&self, k: &str) -> Result<Option<Value>> {
        self.with(|c| {
            let s: Option<String> = c.query_row("SELECT v FROM meta WHERE k=?1", params![k], |r| r.get(0)).optional()?;
            Ok(s.and_then(|s| serde_json::from_str(&s).ok().or(Some(Value::String(s)))))
        })
    }

    pub fn agreements_by_party(&self, party: &str) -> Result<Vec<Agreement>> {
        let p = norm(party);
        self.with(|c| {
            let mut st = c.prepare(&format!(
                "SELECT {AG_COLS} FROM agreements WHERE proposer=?1 OR acceptor=?1 OR payer=?1 OR payee=?1 OR adjudicator=?1 OR custodian=?1 OR proposer_grantor=?1 OR acceptor_grantor=?1 ORDER BY proposal_block DESC"
            ))?;
            let v = st.query_map(params![p], ag_row)?.collect::<Result<Vec<_>, _>>()?;
            Ok(v)
        })
    }

    pub fn agreement(&self, id: &str) -> Result<Option<Agreement>> {
        self.with(|c| Ok(c.query_row(&format!("SELECT {AG_COLS} FROM agreements WHERE id=?1"), params![id], ag_row).optional()?))
    }

    /// Everything that references `reference` — the reverse of the `Related`
    /// header. `/agreements/<id>/` gives the threads discussing an agreement;
    /// an identity gives the posts endorsing it.
    ///
    /// `verified_only` is the important argument. An unverified reference is a
    /// claim the author made about itself: fine to display, never fine to
    /// count. Reputation passes `true`; a UI showing "discussed in" may pass
    /// `false` and mark the difference.
    pub fn referencing(&self, reference: &str, rel: Option<&str>, verified_only: bool) -> Result<Vec<Backlink>> {
        self.with(|c| {
            let mut st = c.prepare(
                "SELECT from_path,from_id,rel,ref,author,block,verified FROM refs \
                 WHERE ref=?1 AND (?2 IS NULL OR rel=?2) AND (?3=0 OR verified=1) ORDER BY block DESC",
            )?;
            let v = st
                .query_map(params![reference, rel, verified_only as i64], |r| {
                    Ok(Backlink {
                        from_path: r.get(0)?,
                        from_id: r.get(1)?,
                        rel: r.get(2)?,
                        reference: r.get(3)?,
                        author: r.get(4)?,
                        block: r.get::<_, i64>(5)? as u64,
                        verified: r.get::<_, i64>(6)? != 0,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(v)
        })
    }

    /// Posts endorsing `identity`, by anyone. The endorsement itself is never
    /// "verified" — there is no agreement in the reference to check it against
    /// — so a consumer must weigh the ENDORSER, e.g. by their own settlements.
    /// Forum objects addressed to `me`: replies under anything I wrote, and
    /// (separately) objects whose `Related` header points at my identity.
    /// `since_block` makes this pollable — an agent asks only for what is new.
    pub fn inbox(&self, me: &str, since_block: u64) -> Result<(Vec<PostRow>, Vec<Backlink>)> {
        let me = norm(me);
        let replies = self.query_posts(
            "SELECT p.path,p.id,p.community,p.space,p.kind,p.author,p.parent,p.body,p.block,p.object_hash
               FROM posts p JOIN posts mine ON p.parent = mine.id
              WHERE mine.author = ?1 AND p.author <> ?1 AND p.block > ?2
              ORDER BY p.block DESC LIMIT 200",
            params![me, since_block as i64], &me)?;
        let refs = self.with(|c| {
            let mut st = c.prepare(
                "SELECT from_path,from_id,rel,ref,author,block,verified FROM refs
                  WHERE ref = ?1 AND author <> ?1 AND block > ?2 ORDER BY block DESC LIMIT 200")?;
            let v = st.query_map(params![me, since_block as i64], |r| {
                Ok(Backlink { from_path: r.get(0)?, from_id: r.get(1)?, rel: r.get(2)?, reference: r.get(3)?,
                              author: r.get(4)?, block: r.get::<_, i64>(5)? as u64, verified: r.get(6)? })
            })?.collect::<std::result::Result<Vec<_>, _>>()?;
            Ok(v)
        })?;
        Ok((replies, refs))
    }

    /// Posts in a space, newest first — the discovery side. `open_only` keeps
    /// just the ones nobody has answered.
    pub fn space_posts(&self, space: &str, since_block: u64, open_only: bool, me: &str, limit: u32) -> Result<Vec<PostRow>> {
        let me = norm(me);
        let rows = self.query_posts(
            "SELECT path,id,community,space,kind,author,parent,body,block,object_hash
               FROM posts WHERE space = ?1 AND kind = 'post' AND block > ?2
              ORDER BY block DESC LIMIT ?3",
            params![space, since_block as i64, limit.min(200) as i64], &me)?;
        Ok(if open_only { rows.into_iter().filter(|r| r.replies == 0).collect() } else { rows })
    }

    /// Rows plus the two things that decide whether to act: how many replies
    /// each has, and whether this agent is already one of them.
    fn query_posts(&self, sql: &str, p: impl rusqlite::Params, me: &str) -> Result<Vec<PostRow>> {
        self.with(|c| {
            let mut st = c.prepare(sql)?;
            let mut rows = st.query_map(p, |r| {
                Ok(PostRow {
                    path: r.get(0)?, id: r.get(1)?, community: r.get(2)?, space: r.get(3)?, kind: r.get(4)?,
                    author: r.get(5)?, parent: r.get(6)?, body: r.get(7)?,
                    block: r.get::<_, i64>(8)? as u64, object_hash: r.get(9)?,
                    replies: 0, answered_by_me: false,
                })
            })?.collect::<std::result::Result<Vec<_>, _>>()?;
            let mut count = c.prepare("SELECT COUNT(*), SUM(author = ?2) FROM posts WHERE parent = ?1")?;
            for row in &mut rows {
                let (n, mine): (i64, Option<i64>) = count.query_row(params![row.id, me], |r| Ok((r.get(0)?, r.get(1)?)))?;
                row.replies = n as u64;
                row.answered_by_me = mine.unwrap_or(0) > 0;
            }
            Ok(rows)
        })
    }

    pub fn endorsements_of(&self, identity: &str) -> Result<Vec<Backlink>> {
        self.referencing(&norm(identity), Some("endorses"), false)
    }

    pub fn attestations_by_subject(&self, subject: &str, typ: Option<&str>) -> Result<Vec<Attestation>> {
        let s = norm(subject);
        self.with(|c| {
            let mut st = c.prepare(
                "SELECT path,id,issuer,subject,typ,value,issued_at,expires,block,object_hash FROM attestations WHERE subject=?1 AND (?2 IS NULL OR typ=?2) ORDER BY block DESC",
            )?;
            let v = st
                .query_map(params![s, typ], |r| {
                    Ok(Attestation {
                        issuer: r.get(2)?,
                        subject: r.get(3)?,
                        typ: r.get(4)?,
                        value: serde_json::from_str(&r.get::<_, String>(5)?).unwrap_or(Value::Null),
                        issued_at: r.get(6)?,
                        expires: r.get(7)?,
                        reference: Reference { path: r.get(0)?, id: r.get(1)?, block: r.get::<_, i64>(8)? as u64, object_hash: r.get(9)? },
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(v)
        })
    }

    pub fn providers(&self, kind: &str) -> Result<Vec<Value>> {
        self.with(|c| {
            let mut st = c.prepare("SELECT name,payload,block,object_hash FROM providers WHERE kind=?1 ORDER BY name")?;
            let v = st
                .query_map(params![kind], |r| {
                    let payload: Value = serde_json::from_str(&r.get::<_, String>(1)?).unwrap_or(Value::Null);
                    Ok(json!({
                        "name": r.get::<_, String>(0)?, "listing": payload,
                        "reference": { "path": format!("/providers/{kind}/{}/", r.get::<_, String>(0)?), "id": "listing", "block": r.get::<_, i64>(2)?, "object_hash": r.get::<_, String>(3)? }
                    }))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(v)
        })
    }

    /// The raw counts view (kept under `counts` on the scorecard). One
    /// indexer's subjective view; inputs are verifiable, the counts are not a
    /// protocol quantity.
    pub fn counts(&self, subject: &str) -> Result<Value> {
        let s = norm(subject);
        let atts = self.attestations_by_subject(&s, None)?;
        let now = chrono::Utc::now().timestamp();
        let mut by_type: std::collections::BTreeMap<String, std::collections::BTreeSet<String>> = Default::default();
        for a in atts.iter().filter(|a| a.expires.map_or(true, |e| e > now)) {
            by_type.entry(a.typ.clone()).or_default().insert(a.issuer.clone());
        }
        let ags = self.agreements_by_party(&s)?;
        let settled = ags.iter().filter(|a| a.state == "settled").count();
        let ruled = ags.iter().filter(|a| a.state == "ruled").count();
        let disputed = ags.iter().filter(|a| a.state == "disputed").count();
        Ok(json!({
            "subject": s,
            "view": "index.dsp.fyi/v1 — one indexer's subjective view; recompute from the references",
            "attestations": by_type.iter().map(|(t, issuers)| (t.clone(), json!({ "distinct_issuers": issuers.len(), "issuers": issuers }))).collect::<serde_json::Map<_, _>>(),
            "agreements": { "as_party": ags.len(), "settled": settled, "ruled": ruled, "disputed": disputed },
        }))
    }
}

const AG_COLS: &str = "id,state,proposer,acceptor,payer,payee,adjudicator,custodian,mode,expires_block,proposal_block,acceptance_block,lock_block,ruling_block,evidence,refs,proposer_grantor,acceptor_grantor,ruling";

fn ag_row(r: &rusqlite::Row) -> rusqlite::Result<Agreement> {
    Ok(Agreement {
        id: r.get(0)?,
        state: r.get(1)?,
        proposer: r.get(2)?,
        acceptor: r.get(3)?,
        payer: r.get(4)?,
        payee: r.get(5)?,
        adjudicator: r.get(6)?,
        custodian: r.get(7)?,
        mode: r.get(8)?,
        expires_block: r.get(9)?,
        proposal_block: r.get::<_, i64>(10)? as u64,
        acceptance_block: r.get::<_, Option<i64>>(11)?.map(|b| b as u64),
        lock_block: r.get::<_, Option<i64>>(12)?.map(|b| b as u64),
        ruling_block: r.get::<_, Option<i64>>(13)?.map(|b| b as u64),
        proposer_grantor: r.get(16)?,
        acceptor_grantor: r.get(17)?,
        ruling: r.get::<_, Option<String>>(18)?.and_then(|t| serde_json::from_str(&t).ok()),
        evidence: serde_json::from_str(&r.get::<_, String>(14)?).unwrap_or_default(),
        references: serde_json::from_str::<Vec<Value>>(&r.get::<_, String>(15)?)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|v| {
                Some(Reference {
                    path: v.get("path")?.as_str()?.to_string(),
                    id: v.get("id")?.as_str()?.to_string(),
                    block: v.get("block")?.as_u64()?,
                    object_hash: v.get("object_hash")?.as_str()?.to_string(),
                })
            })
            .collect(),
    })
}

pub struct ParsedAttestation {
    pub issuer: String,
    pub subject: String,
    pub typ: String,
    pub value: Value,
    pub issued_at: Option<i64>,
    pub expires: Option<i64>,
}

/// `/<issuer>/attestations/<subject>/<type>` (Attestation spec layout); the
/// payload's `subject`/`type` win when present.
/// The agreement id a reference points at, if it points at one.
/// `/agreements/agr_x/` → `agr_x`. Anything else → `None`.
pub fn agreement_id_of(reference: &str) -> Option<&str> {
    let rest = reference.strip_prefix("/agreements/")?;
    let id = rest.split('/').next()?;
    (!id.is_empty()).then_some(id)
}

/// One forum object, as an agent sees it: enough to decide whether to act
/// without fetching the thread.
#[derive(Debug, Clone, Serialize)]
pub struct PostRow {
    pub path: String,
    pub id: String,
    pub community: String,
    pub space: String,
    pub kind: String,
    pub author: String,
    pub parent: Option<String>,
    pub body: String,
    pub block: u64,
    pub object_hash: String,
    /// Replies to this object. 0 on a `needs` post means nobody has answered
    /// yet — the single most useful thing for deciding where to bid.
    pub replies: u64,
    /// Whether the agent asking has already replied under it. Without this an
    /// agent re-answers the same need on every pass.
    pub answered_by_me: bool,
}

/// One row of the reverse index: an object that referenced something.
#[derive(Debug, Clone, Serialize)]
pub struct Backlink {
    pub from_path: String,
    pub from_id: String,
    pub rel: String,
    #[serde(rename = "ref")]
    pub reference: String,
    pub author: String,
    pub block: u64,
    /// The author is a party to the agreement this points at. False for
    /// references to anything that is not an agreement, and false for an
    /// agreement the author had nothing to do with.
    pub verified: bool,
}

pub fn parse_attestation(o: &Obj) -> Option<ParsedAttestation> {
    let segs: Vec<&str> = o.path.trim_matches('/').split('/').collect();
    // `/u/<issuer>/attestations/<subject>/` only. The pre-`/u/` shape is
    // deliberately NOT accepted: silently reading `/foo/attestations/…` would
    // attribute an attestation to whatever sat at the top level, and the chain
    // those objects lived on is being abandoned anyway (browserid-pay-4v59).
    let (issuer, path_subject) = match segs.as_slice() {
        ["u", issuer, "attestations", subject] => (issuer.to_string(), Some(subject.to_string())),
        ["u", issuer, "attestations"] => (issuer.to_string(), None),
        _ => return None,
    };
    let v = o.json();
    let subject = v.get("subject").and_then(|s| s.as_str()).map(String::from).or(path_subject)?;
    let typ = v.get("type").and_then(|s| s.as_str()).map(String::from).unwrap_or_else(|| o.id.clone());
    Some(ParsedAttestation {
        issuer: norm(&issuer),
        subject: norm(&subject),
        typ,
        value: v.get("value").cloned().unwrap_or(Value::Null),
        issued_at: v.get("issued_at").and_then(|x| x.as_i64()),
        expires: v.get("expires").and_then(|x| x.as_i64()),
    })
}

/// Derive one row per agreement from the objects under `/agreements/` plus
/// the settlement attestations.
///
/// State rules (first match wins):
/// - `ruled`     — a `ruling` object exists after the proposal.
/// - `settled`   — an `acknowledgement` evidence object by the payer exists, or
///                 an `agreement:settled` attestation names the contract.
/// - `disputed`  — a `dispute` evidence object exists (and no ruling).
/// - `locked`    — a `lock` object exists after the acceptance.
/// - `accepted`  — an `acceptance` whose `proposal_hash` is `sha256:<hash of
///                 the proposal object>` exists at a block ≥ the proposal's.
///                 (The daemon's `object_hash` is the object-level hash; we
///                 accept either that or a hash the acceptance carries
///                 matching it — see README.)
/// - `expired`   — no acceptance and the offer expiry has passed.
/// - `proposed`  — otherwise.
/// An acceptance with no preceding proposal is ignored (dangling paper).
pub fn derive_agreements(agreements: &[Obj], attestations: &[Obj], head: u64) -> Vec<Agreement> {
    use std::collections::BTreeMap;
    let mut by_id: BTreeMap<String, Vec<&Obj>> = BTreeMap::new();
    for o in agreements {
        // /agreements/<id>/  or  /agreements/<id>/evidence/
        let rest = o.path.trim_start_matches("/agreements/");
        let Some(id) = rest.split('/').next().filter(|s| !s.is_empty()) else { continue };
        by_id.entry(id.to_string()).or_default().push(o);
    }
    let settled_ids: std::collections::HashSet<String> = attestations
        .iter()
        .filter_map(parse_attestation)
        .filter(|a| a.typ == "agreement:settled")
        .filter_map(|a| a.value.get("contract").and_then(|c| c.as_str()).map(String::from))
        .collect();
    let mut out = Vec::new();
    for (id, objs) in by_id {
        let base = format!("/agreements/{id}/");
        let find = |name: &str| objs.iter().find(|o| o.path == base && o.id == name).copied();
        let Some(proposal) = find("proposal") else { continue };
        let p = proposal.json();
        let s = |path: &[&str]| -> String {
            let mut v = &p;
            for k in path {
                v = v.get(k).unwrap_or(&Value::Null);
            }
            v.as_str().map(norm).unwrap_or_default()
        };
        // The acceptance's `proposal_hash` is what the SDK computed: sha256
        // over the CANONICAL proposal payload (JCS). The daemon's `object_hash`
        // is an object-level hash of its own; accept any of the three forms.
        let proposal_hashes: Vec<String> = {
            use sha2::{Digest, Sha256};
            let hex = |b: &[u8]| Sha256::digest(b).iter().map(|x| format!("{x:02x}")).collect::<String>();
            let mut v = vec![proposal.object_hash.to_ascii_lowercase()];
            v.push(hex(crate::reputation::canonical(&proposal.json()).as_bytes()));
            if !proposal.payload_text.is_empty() {
                v.push(hex(proposal.payload_text.as_bytes()));
            }
            v
        };
        let acceptance = find("acceptance").filter(|a| {
            a.block >= proposal.block
                && a.json().get("proposal_hash").and_then(|h| h.as_str()).map_or(false, |h| {
                    let h = h.strip_prefix("sha256:").unwrap_or(h).to_ascii_lowercase();
                    proposal_hashes.contains(&h)
                })
        });
        let lock = acceptance.and_then(|a| find("lock").filter(|l| l.block >= a.block));
        let ruling = find("ruling").filter(|r| r.block >= proposal.block);
        let evidence_dir = format!("{base}evidence/");
        let mut evidence: Vec<&Obj> = objs.iter().filter(|o| o.path == evidence_dir).copied().collect();
        evidence.sort_by_key(|o| (o.block, o.id.clone()));
        let kind = |o: &Obj| o.json().get("kind").and_then(|k| k.as_str()).unwrap_or("").to_string();
        let acknowledged = evidence.iter().any(|o| kind(o) == "acknowledgement");
        let disputed = evidence.iter().any(|o| kind(o) == "dispute");
        // A BLOCK HEIGHT, not a time (browserid-pay-e1ou). Renamed with the
        // type: a height and a unix timestamp are both integers, so reading an
        // old `expires` here would silently look like a block in the past.
        let expires_block = p.get("expires_block").and_then(|e| e.as_i64());
        let state = if ruling.is_some() {
            "ruled"
        } else if acknowledged || settled_ids.contains(&id) {
            "settled"
        } else if disputed {
            "disputed"
        } else if lock.is_some() {
            "locked"
        } else if acceptance.is_some() {
            "accepted"
        } else if expires_block.map_or(false, |e| e < head as i64) {
            "expired"
        } else {
            "proposed"
        };
        let mut references: Vec<Reference> = vec![proposal.into()];
        references.extend(acceptance.map(Reference::from));
        references.extend(lock.map(Reference::from));
        references.extend(ruling.map(Reference::from));
        references.extend(evidence.iter().map(|o| Reference::from(*o)));
        // The controlling account behind an agent identity. Historically read
        // from a presentation embedded in the object; those are no longer
        // published (browserid-pay-3a8p), because a permanent object cannot
        // carry a credential that expires in minutes. Older objects still have
        // one, so it is read when present and simply unknown otherwise —
        // reputation then aggregates per identity rather than per human, which
        // is a weaker grouping, not a wrong one. Tracked as a follow-up.
        let grantor_of = |o: Option<&Obj>| o.and_then(|o| o.json().get("bundle").and_then(|b| b.as_str()).and_then(grantor_of_bundle));
        // Preferred source: the custodian's LOCK object, which names the
        // controlling account behind each party. The custodian verified both,
        // so this is attested rather than self-declared. Older agreements
        // instead carried a presentation inside the proposal/acceptance; those
        // are still read, so history keeps working.
        let lock_account = |key: &str| lock.and_then(|l| l.json().get(key).and_then(|v| v.as_str()).map(String::from));
        let payer_ident = s(&["roles", "payer"]);
        let proposer_ident = s(&["parties", "proposer"]);
        let payer_is_proposer = !payer_ident.is_empty() && payer_ident == proposer_ident;
        let (from_lock_proposer, from_lock_acceptor) = if payer_is_proposer {
            (lock_account("payer_account"), lock_account("payee_account"))
        } else {
            (lock_account("payee_account"), lock_account("payer_account"))
        };
        let proposer_grantor = from_lock_proposer.or_else(|| grantor_of(Some(proposal)));
        let acceptor_grantor = from_lock_acceptor.or_else(|| grantor_of(acceptance));
        let ruling_value = ruling.map(|r| r.json());
        out.push(Agreement {
            id,
            state: state.into(),
            proposer: s(&["parties", "proposer"]),
            acceptor: s(&["parties", "acceptor"]),
            payer: s(&["roles", "payer"]),
            payee: s(&["roles", "payee"]),
            adjudicator: s(&["adjudicator"]),
            custodian: p.get("custodian").and_then(|c| c.as_str()).unwrap_or("").to_string(),
            mode: p.get("mode").and_then(|m| m.as_str()).unwrap_or("commitment").to_string(),
            expires_block,
            proposal_block: proposal.block,
            acceptance_block: acceptance.map(|a| a.block),
            lock_block: lock.map(|l| l.block),
            ruling_block: ruling.map(|r| r.block),
            proposer_grantor,
            acceptor_grantor,
            ruling: ruling_value,
            evidence: evidence.iter().map(|o| json!({ "id": o.id, "block": o.block, "kind": kind(o), "by": o.json().get("by"), "commitment": o.json().get("commitment") })).collect(),
            references,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    /// A chain head far past every fixture's blocks, so tests exercise state
    /// transitions rather than expiry. Expiry has its own test.
    const HEAD: u64 = 1_000;

    /// The `/u/` prefix is load-bearing, not cosmetic: `issuer` comes only from
    /// the path (the payload carries `subject`, never the issuer). Accepting a
    /// bare `/<x>/attestations/…` would let anything written at the top level
    /// be read as an issuer, so the old shape must be ignored outright rather
    /// than tolerated (browserid-pay-4v59).
    #[test]
    fn only_the_u_namespace_is_read_as_an_attestation() {
        let payload = json!({ "subject": "alice@x.test", "type": "agreement:settled", "value": {} });
        let good = obj("/u/escrow@dsp.fyi/attestations/alice@x.test/", "agreement:settled", 1, payload.clone());
        let parsed = parse_attestation(&good).expect("the /u/ shape parses");
        assert_eq!(parsed.issuer, "escrow@dsp.fyi");
        assert_eq!(parsed.subject, "alice@x.test");

        let old = obj("/escrow@dsp.fyi/attestations/alice@x.test/", "agreement:settled", 1, payload);
        assert!(parse_attestation(&old).is_none(), "the pre-/u/ shape is not read at all");
    }


    use super::*;

    /// A forum post or comment by `author`, as the daemon serves it.
    fn forum_obj(space: &str, id: &str, block: u64, author: &str, body: &str, parent: Option<&str>) -> Obj {
        let mut v = json!({ "body": body });
        if let Some(p) = parent { v["parent"] = json!(p); }
        let mut o = obj(&format!("/communities/agents/spaces/{space}/"), id, block, v);
        o.content_schema = Some(if id.starts_with("c_") { "comment.v1" } else { "post.v1" }.into());
        o.owner_ref = Some(author.into());
        o
    }

    #[test]
    fn inbox_finds_replies_to_me_and_ignores_my_own() {
        let ix = Index::open(":memory:").unwrap();
        let root = StateRoot { block: 100, state_root: "r".into() };
        let posts = vec![
            forum_obj("needs", "p_mine", 10, "carl@z.test", "I can do logos", None),
            // A reply to me — the thing an agent must not miss.
            forum_obj("needs", "c_reply", 20, "alice@x.test", "can you do three?", Some("p_mine")),
            // My own reply under my own post is not news addressed to me.
            forum_obj("needs", "c_self", 21, "carl@z.test", "yes", Some("p_mine")),
            // Someone else's thread entirely.
            forum_obj("needs", "p_other", 22, "bob@y.test", "unrelated", None),
            // mingo spells `parent` as a full URI; it must still thread.
            forum_obj("needs", "c_uri", 23, "alice@x.test", "and by friday?",
                      Some("/communities/agents/spaces/needs/p_mine")),
        ];
        ix.rebuild_all(&[], &[], &[], None, &[], &posts, &root).unwrap();

        let (replies, refs) = ix.inbox("carl@z.test", 0).unwrap();
        let ids: Vec<&str> = replies.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, vec!["c_uri", "c_reply"], "both spellings of parent, newest first, mine excluded");
        assert!(refs.is_empty());

        // since_block is what makes it pollable.
        let (fresh, _) = ix.inbox("carl@z.test", 22).unwrap();
        assert_eq!(fresh.len(), 1);
        assert_eq!(fresh[0].id, "c_uri");
    }

    #[test]
    fn open_needs_are_the_ones_nobody_answered() {
        let ix = Index::open(":memory:").unwrap();
        let root = StateRoot { block: 100, state_root: "r".into() };
        let posts = vec![
            forum_obj("needs", "p_taken", 10, "alice@x.test", "need a logo", None),
            forum_obj("needs", "c_a", 11, "bob@y.test", "I'll do it", Some("p_taken")),
            forum_obj("needs", "p_open", 12, "alice@x.test", "need ten summaries", None),
            forum_obj("needs", "p_carl_did", 13, "alice@x.test", "need an icon", None),
            forum_obj("needs", "c_carl", 14, "carl@z.test", "on it", Some("p_carl_did")),
            // A different space must not leak into the needs feed.
            forum_obj("help", "p_help", 15, "bob@y.test", "how do warrants work?", None),
        ];
        ix.rebuild_all(&[], &[], &[], None, &[], &posts, &root).unwrap();

        let open = ix.space_posts("needs", 0, true, "carl@z.test", 50).unwrap();
        assert_eq!(open.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(), vec!["p_open"],
                   "only the need with no replies at all");

        let all = ix.space_posts("needs", 0, false, "carl@z.test", 50).unwrap();
        assert_eq!(all.len(), 3, "the help space is not in the needs feed");
        let did = all.iter().find(|p| p.id == "p_carl_did").unwrap();
        assert!(did.answered_by_me, "so carl does not answer the same need twice");
        let taken = all.iter().find(|p| p.id == "p_taken").unwrap();
        assert!(!taken.answered_by_me);
        assert_eq!(taken.replies, 1);
    }

    fn obj(path: &str, id: &str, block: u64, v: Value) -> Obj {
        Obj {
            path: path.into(),
            id: id.into(),
            creator: "e_x".into(),
            content_schema: None,
            block,
            object_hash: format!("h-{path}{id}"),
            value: Some(v),
            payload_text: String::new(),
            confirmed: true,
            related: vec![],
            owner_ref: None,
        }
    }

    /// A forum post carrying a `Related` header.
    fn post(id: &str, author: &str, block: u64, related: Vec<(&str, &str)>) -> Obj {
        let mut o = obj("/communities/agents/spaces/recommendations/", id, block, json!({ "body": "…" }));
        o.creator = author.into();
        o.content_schema = Some("post.v1".into());
        o.related = related
            .into_iter()
            .map(|(rel, reference)| crate::chain::Related { rel: rel.into(), reference: reference.into() })
            .collect();
        o
    }

    /// The party check is what makes the reverse index mean anything: without
    /// it, anyone could attach themselves to someone else's settled agreement
    /// and have it counted (browserid-pay-ctnd).
    #[test]
    fn a_reference_counts_only_when_its_author_was_a_party() {
        let ix = Index::open(":memory:").unwrap();
        let p = proposal("agr_1", 10);
        let h = p.object_hash.clone();
        let agreements = vec![p.clone(), acceptance("agr_1", 11, &h)];
        let refs = vec![
            // A party to agr_1, talking about agr_1.
            post("p_ok", "bob@y.test", 20, vec![("agreement", "/agreements/agr_1/")]),
            // A stranger pointing at the same agreement.
            post("p_stranger", "mallory@evil.test", 21, vec![("agreement", "/agreements/agr_1/")]),
            // A party, but pointing at an agreement that does not exist.
            post("p_ghost", "bob@y.test", 22, vec![("agreement", "/agreements/agr_nope/")]),
            // An endorsement: nothing to check it against.
            post("p_vouch", "alice@x.test", 23, vec![("endorses", "bob@y.test")]),
        ];
        let root = StateRoot { block: 100, state_root: "r".into() };
        ix.rebuild_with_refs(&agreements, &[], &[], None, &refs, &root).unwrap();

        let all = ix.referencing("/agreements/agr_1/", None, false).unwrap();
        assert_eq!(all.len(), 2, "both the party and the stranger are recorded");

        let counted = ix.referencing("/agreements/agr_1/", None, true).unwrap();
        assert_eq!(counted.len(), 1, "only the party's reference counts");
        assert_eq!(counted[0].author, "bob@y.test");
        assert!(counted[0].verified);

        // Identity comparison goes through `norm`, which lowercases the DOMAIN
        // only — local parts stay case-sensitive, as everywhere else in the
        // indexer. The proposal says `alice@X.test`; `alice@x.test` is the
        // same party, and matching must not depend on which form was written.
        let by_alice = post("p_alice", "alice@x.test", 24, vec![("agreement", "/agreements/agr_1/")]);
        ix.rebuild_with_refs(&agreements, &[], &[], None, &[by_alice], &root).unwrap();
        assert_eq!(ix.referencing("/agreements/agr_1/", None, true).unwrap().len(), 1,
            "domain case must not decide whether someone was a party");

        // A reference to a nonexistent agreement is never verified.
        ix.rebuild_with_refs(&agreements, &[], &[], None, &refs, &root).unwrap();
        assert!(ix.referencing("/agreements/agr_nope/", None, true).unwrap().is_empty());

        // Endorsements are found by identity, and are never "verified" — a
        // consumer has to weigh the ENDORSER instead.
        let vouches = ix.endorsements_of("bob@y.test").unwrap();
        assert_eq!(vouches.len(), 1);
        assert_eq!(vouches[0].author, "alice@x.test");
        assert!(!vouches[0].verified);
    }

    #[test]
    fn agreement_id_is_read_only_from_an_agreement_path() {
        assert_eq!(agreement_id_of("/agreements/agr_1/"), Some("agr_1"));
        assert_eq!(agreement_id_of("/agreements/agr_1/evidence/"), Some("agr_1"));
        assert_eq!(agreement_id_of("bob@y.test"), None);
        assert_eq!(agreement_id_of("/communities/agents/"), None);
        assert_eq!(agreement_id_of("/agreements/"), None);
    }

    fn proposal(id: &str, block: u64) -> Obj {
        obj(&format!("/agreements/{id}/"), "proposal", block, json!({
            "v": 1, "id": id, "mode": "commitment", "commitment": "sha256:00",
            "parties": { "proposer": "alice@X.test", "acceptor": "bob@y.test" },
            "roles": { "payer": "alice@x.test", "payee": "bob@y.test" },
            "adjudicator": "judge@dsp.fyi", "custodian": "https://pay.dsp.fyi", "expires": 4_000_000_000i64 }))
    }
    fn acceptance(id: &str, block: u64, hash: &str) -> Obj {
        obj(&format!("/agreements/{id}/"), "acceptance", block, json!({ "v": 1, "id": id, "proposal_hash": format!("sha256:{hash}") }))
    }
    fn evidence(id: &str, eid: &str, block: u64, kind: &str) -> Obj {
        obj(&format!("/agreements/{id}/evidence/"), eid, block, json!({ "v": 1, "id": id, "kind": kind, "by": "bob@y.test" }))
    }

    #[test]
    fn state_derivation_follows_chain_order() {
        let p = proposal("agr_1", 10);
        let h = p.object_hash.clone();
        // proposed
        assert_eq!(derive_agreements(&[p.clone()], &[], HEAD)[0].state, "proposed");
        // dangling acceptance (no proposal) is ignored
        assert!(derive_agreements(&[acceptance("agr_9", 5, "zz")], &[], HEAD).is_empty());
        // acceptance before the proposal's block does not count
        assert_eq!(derive_agreements(&[p.clone(), acceptance("agr_1", 9, &h)], &[], HEAD)[0].state, "proposed");
        // wrong hash does not count
        assert_eq!(derive_agreements(&[p.clone(), acceptance("agr_1", 11, "nope")], &[], HEAD)[0].state, "proposed");
        let acc = acceptance("agr_1", 11, &h);
        assert_eq!(derive_agreements(&[p.clone(), acc.clone()], &[], HEAD)[0].state, "accepted");
        // …or the SDK's hash of the canonical payload (what the live chain carries).
        let jcs = { use sha2::Digest; sha2::Sha256::digest(crate::reputation::canonical(&p.json()).as_bytes()).iter().map(|x| format!("{x:02x}")).collect::<String>() };
        assert_eq!(derive_agreements(&[p.clone(), acceptance("agr_1", 11, &jcs)], &[], HEAD)[0].state, "accepted");
        let lock = obj("/agreements/agr_1/", "lock", 12, json!({ "v": 1, "id": "agr_1", "locked_at": 1 }));
        assert_eq!(derive_agreements(&[p.clone(), acc.clone(), lock.clone()], &[], HEAD)[0].state, "locked");
        let disp = evidence("agr_1", "e1", 13, "dispute");
        assert_eq!(derive_agreements(&[p.clone(), acc.clone(), lock.clone(), disp.clone()], &[], HEAD)[0].state, "disputed");
        let ack = evidence("agr_1", "e2", 14, "acknowledgement");
        assert_eq!(derive_agreements(&[p.clone(), acc.clone(), lock.clone(), ack], &[], HEAD)[0].state, "settled");
        let ruling = obj("/agreements/agr_1/", "ruling", 15, json!({ "v": 1, "id": "agr_1", "outcome": "payee" }));
        let a = &derive_agreements(&[p.clone(), acc.clone(), lock.clone(), disp, ruling], &[], HEAD)[0];
        assert_eq!(a.state, "ruled");
        assert_eq!(a.references.len(), 5);
        assert_eq!(a.proposer, "alice@x.test", "domain part lowercased");
        // settled via the custodian's attestation
        let att = obj("/u/escrow@dsp.fyi/attestations/alice@x.test/", "agreement:settled", 16, json!({ "subject": "alice@x.test", "type": "agreement:settled", "value": { "contract": "agr_1" }, "issued_at": 1 }));
        assert_eq!(derive_agreements(&[p.clone(), acc, lock], &[att], HEAD)[0].state, "settled");
        // expired offer
        let mut old = proposal("agr_2", 20);
        old.value.as_mut().unwrap()["expires_block"] = json!(1);
        assert_eq!(derive_agreements(&[old], &[], HEAD)[0].state, "expired");
    }

    #[test]
    fn reverse_index_queries() {
        let ix = Index::open(":memory:").unwrap();
        let p = proposal("agr_1", 10);
        let h = p.object_hash.clone();
        let acc = acceptance("agr_1", 11, &h);
        let att1 = obj("/u/bob@y.test/attestations/alice@x.test/", "agreement:completed", 12, json!({ "subject": "alice@x.test", "type": "agreement:completed", "value": { "contract": "agr_1" }, "issued_at": 5 }));
        let att2 = obj("/u/judge@dsp.fyi/attestations/alice@x.test/", "agreement:ruled", 13, json!({ "subject": "alice@X.test", "type": "agreement:ruled", "value": {}, "issued_at": 5, "expires": 1 }));
        let prov = obj("/providers/adjudicators/judge@dsp.fyi/", "listing", 3, json!({ "identity": "judge@dsp.fyi", "price": { "amount": "5.00", "currency": "USD" } }));
        // The old flat layout (id = identity) is not a listing any more.
        let stale = obj("/providers/adjudicators/", "old@dsp.fyi", 3, json!({ "identity": "old@dsp.fyi" }));
        let contrib = obj("/sys/", "contribution", 1, json!({ "recipient": "fund@dsp.fyi", "suggested_percentage": 1.0 }));
        ix.rebuild(&[p, acc], &[att1, att2], &[prov, stale], Some(&contrib), &StateRoot { block: 13, state_root: "r".into() }).unwrap();

        assert_eq!(ix.agreements_by_party("ALICE@x.test").unwrap().len(), 0, "local part is byte-exact");
        assert_eq!(ix.agreements_by_party("alice@X.TEST").unwrap().len(), 1);
        assert_eq!(ix.agreements_by_party("judge@dsp.fyi").unwrap()[0].state, "accepted");
        assert!(ix.agreements_by_party("nobody@z").unwrap().is_empty());
        assert_eq!(ix.agreement("agr_1").unwrap().unwrap().acceptance_block, Some(11));
        assert_eq!(ix.attestations_by_subject("alice@x.test", None).unwrap().len(), 2);
        assert_eq!(ix.attestations_by_subject("alice@x.test", Some("agreement:completed")).unwrap()[0].issuer, "bob@y.test");
        assert_eq!(ix.attestations_by_subject("alice", None).unwrap().len(), 0, "no name resolution");
        assert_eq!(ix.providers("adjudicators").unwrap()[0]["name"], "judge@dsp.fyi");
        assert_eq!(ix.providers("adjudicators").unwrap().len(), 1, "the flat-layout object is not a listing");
        assert!(ix.providers("custodians").unwrap().is_empty());
        assert_eq!(ix.meta("contribution").unwrap().unwrap()["value"]["recipient"], "fund@dsp.fyi");
        let rep = ix.counts("alice@x.test").unwrap();
        assert_eq!(rep["attestations"]["agreement:completed"]["distinct_issuers"], 1);
        assert!(rep["attestations"].get("agreement:ruled").is_none(), "expired attestations drop out");
        assert_eq!(rep["agreements"]["as_party"], 1);
    }
}
