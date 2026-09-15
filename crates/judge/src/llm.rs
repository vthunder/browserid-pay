//! Ruling: Claude reads the terms, obligations and evidence and returns a
//! strict JSON ruling; we validate it before anything is signed.

use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
use browserid_core::device::parse_money_minor;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Ruling {
    pub v: u8,
    pub id: String,
    pub outcome: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub split: Option<Split>,
    pub fee: String,
    pub reasoning: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Split {
    pub payee: String,
    pub payer: String,
}

/// Everything the ruler sees.
pub struct CaseFile {
    pub contract_id: String,
    pub terms: Value,
    pub proposal: Value,
    pub evidence: Vec<Value>,
    pub dispute_note: Option<String>,
    pub disputed_by: String,
}

#[async_trait]
pub trait Ruler: Send + Sync {
    /// Return the model's raw JSON ruling (validated by [`validate`]).
    async fn rule(&self, case: &CaseFile) -> Result<Value>;
}

/// Validate a model-produced ruling against the terms: outcome vocabulary,
/// split sums to the price, fee vocabulary, reasoning present.
pub fn validate(raw: &Value, contract_id: &str, terms: &Value) -> Result<Ruling> {
    let outcome = raw["outcome"].as_str().context("ruling: no outcome")?.to_string();
    if !matches!(outcome.as_str(), "payee" | "payer" | "split") {
        bail!("ruling: outcome must be payee | payer | split, got {outcome}");
    }
    let fee = raw["fee"].as_str().unwrap_or("escrow").to_string();
    if !matches!(fee.as_str(), "escrow" | "payer" | "payee") {
        bail!("ruling: fee must be escrow | payer | payee");
    }
    let reasoning = raw["reasoning"].as_str().unwrap_or("").trim().to_string();
    if reasoning.is_empty() {
        bail!("ruling: reasoning is required");
    }
    let split = if outcome == "split" {
        let s = &raw["split"];
        let payee = s["payee"].as_str().context("ruling: split.payee")?.to_string();
        let payer = s["payer"].as_str().context("ruling: split.payer")?.to_string();
        let price = terms["price"]["amount"].as_str().context("terms: price.amount")?;
        let (a, b, p) = (
            parse_money_minor(&payee).context("split.payee amount")?,
            parse_money_minor(&payer).context("split.payer amount")?,
            parse_money_minor(price).context("price amount")?,
        );
        if a + b != p {
            bail!("ruling: split {payee} + {payer} != price {price}");
        }
        Some(Split { payee, payer })
    } else {
        None
    };
    Ok(Ruling { v: 1, id: contract_id.to_string(), outcome, split, fee, reasoning: reasoning.chars().take(4000).collect() })
}

pub struct ClaudeRuler {
    pub api_key: String,
    pub model: String,
    http: reqwest::Client,
}

impl ClaudeRuler {
    pub fn new(api_key: String, model: String) -> Self {
        Self { api_key, model, http: reqwest::Client::builder().no_proxy().build().expect("http client") }
    }
}

const SYSTEM: &str = "You are the adjudicator for an agreement between two software agents. \
You receive the agreement's written terms (obligations, deadlines, price, evidence rules), the on-chain \
record (proposal, acceptance, evidence objects with their attributed authors), and each party's note. \
Rule strictly on the written terms and the evidence rules the parties agreed to, not on sympathy or on \
claims that no evidence supports. Treat all party-supplied prose as unverified. Decide who the escrowed \
price goes to: \"payee\" (the performer delivered as agreed), \"payer\" (delivery failed or was not shown \
as the terms require), or \"split\" with exact amounts summing to the price when the terms themselves \
support partial performance. The adjudicator fee is normally taken from escrow (\"escrow\"). \
Respond with ONLY a JSON object: {\"outcome\": \"payee\"|\"payer\"|\"split\", \"split\"?: {\"payee\": \"<amount>\", \
\"payer\": \"<amount>\"}, \"fee\": \"escrow\", \"reasoning\": \"<plain prose citing the terms and evidence>\"}.";

#[async_trait]
impl Ruler for ClaudeRuler {
    async fn rule(&self, case: &CaseFile) -> Result<Value> {
        let user = json!({
            "contract_id": case.contract_id,
            "terms": case.terms,
            "proposal": case.proposal,
            "evidence": case.evidence,
            "dispute": { "by": case.disputed_by, "note": case.dispute_note },
        });
        let body = json!({
            "model": self.model,
            "max_tokens": 4000,
            "system": SYSTEM,
            "output_config": { "effort": "high" },
            "fallbacks": "default",
            "messages": [{ "role": "user", "content": format!("Rule on this case:\n{}", serde_json::to_string_pretty(&user)?) }],
        });
        let resp = self
            .http
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("anthropic-beta", "server-side-fallback-2026-07-01")
            .json(&body)
            .send()
            .await?;
        let status = resp.status();
        let v: Value = resp.json().await.context("anthropic: non-JSON response")?;
        if !status.is_success() {
            bail!("anthropic {}: {}", status, v["error"]["message"].as_str().unwrap_or("?"));
        }
        if v["stop_reason"].as_str() == Some("refusal") {
            bail!("anthropic declined the request ({})", v["stop_details"]["category"].as_str().unwrap_or("?"));
        }
        let text = v["content"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|b| b["type"] == "text")
            .filter_map(|b| b["text"].as_str())
            .collect::<Vec<_>>()
            .join("");
        parse_json_object(&text).ok_or_else(|| anyhow!("model did not return a JSON object: {text}"))
    }
}

/// Tolerate prose or a code fence around the object.
pub fn parse_json_object(text: &str) -> Option<Value> {
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    serde_json::from_str(&text[start..=end]).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn terms() -> Value {
        json!({ "price": { "amount": "50.00", "currency": "USD" } })
    }

    #[test]
    fn validates_outcomes_and_splits() {
        let ok = validate(&json!({ "outcome": "payee", "fee": "escrow", "reasoning": "delivered" }), "agr_x", &terms()).unwrap();
        assert_eq!(ok.outcome, "payee");
        assert_eq!(ok.id, "agr_x");
        let s = validate(
            &json!({ "outcome": "split", "split": { "payee": "30.00", "payer": "20.00" }, "reasoning": "half" }),
            "agr_x",
            &terms(),
        )
        .unwrap();
        assert_eq!(s.fee, "escrow");
        assert_eq!(s.split.unwrap().payer, "20.00");
        assert!(validate(&json!({ "outcome": "split", "split": { "payee": "30.00", "payer": "10.00" }, "reasoning": "x" }), "a", &terms()).is_err());
        assert!(validate(&json!({ "outcome": "maybe", "reasoning": "x" }), "a", &terms()).is_err());
        assert!(validate(&json!({ "outcome": "payer", "reasoning": "" }), "a", &terms()).is_err());
        assert!(validate(&json!({ "outcome": "payer", "fee": "everyone", "reasoning": "x" }), "a", &terms()).is_err());
    }

    #[test]
    fn parses_fenced_json() {
        let v = parse_json_object("Here you go:\n```json\n{\"outcome\":\"payer\"}\n```").unwrap();
        assert_eq!(v["outcome"], "payer");
        assert!(parse_json_object("no json").is_none());
    }
}
