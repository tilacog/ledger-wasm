//! "ledger CLI in the browser" MVP: ledger/hledger/beancount journals are
//! parsed and validated by doppio (parse -> elaborate, with double-entry
//! checking), balance/register reports are computed here in Rust, and the
//! whole engine is exposed to JavaScript via wasm-bindgen. No `.dop` files.

use std::collections::BTreeMap;

use rust_decimal::Decimal;
use serde::Serialize;
use wasm_bindgen::prelude::*;

use doppio::elaboration::Journal;

// ---------------------------------------------------------------------------
// Data model returned to JS (amounts are pre-formatted strings)
// ---------------------------------------------------------------------------

/// One account row in a balance report, already folded over descendants.
#[derive(Serialize)]
pub struct BalanceRow {
    /// Full path, e.g. `Assets:Checking` (parents precede children).
    pub account: String,
    /// Depth = colon count; the JS side uses it for indentation.
    pub depth: u32,
    /// One formatted string per commodity, e.g. `"$4024.80"` or `"5 AAPL"`.
    pub amounts: Vec<String>,
}

#[derive(Serialize)]
pub struct BalanceReport {
    pub rows: Vec<BalanceRow>,
    /// Global total per commodity across all root-level accounts.
    pub total: Vec<String>,
}

#[derive(Serialize)]
pub struct RegisterPosting {
    pub account: String,
    pub commodity: String,
    /// Decimal serialized as a string so JS never sees float rounding.
    pub amount: String,
}

#[derive(Serialize)]
pub struct RegisterTxn {
    /// `YYYY-MM-DD`
    pub date: String,
    /// `cleared` | `pending` | `unmarked`
    pub state: String,
    pub description: String,
    pub postings: Vec<RegisterPosting>,
}

#[derive(Serialize)]
pub struct RegisterReport {
    pub transactions: Vec<RegisterTxn>,
}

// ---------------------------------------------------------------------------
// Engine
// ---------------------------------------------------------------------------

/// Holds the elaborated journal (postings fully evaluated, null postings
/// filled in, balance assertions checked by doppio during elaboration).
#[wasm_bindgen]
pub struct Engine {
    journal: Journal,
}

fn to_js_error(msg: String) -> JsError {
    // pest parse errors embed ` --> LINE:COL`; surface it prominently so the
    // UI can point at the offending line, mirroring doppio-wasm's behaviour.
    let annotated = msg.find(" --> ").and_then(|start| {
        let rest = &msg[start + 5..];
        let colon = rest.find(':')?;
        let line: u32 = rest[..colon].trim().parse().ok()?;
        let after = &rest[colon + 1..];
        let end = after
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(after.len());
        let col: u32 = after[..end].trim().parse().ok()?;
        Some(format!("parse error (line {line}, col {col}): {msg}"))
    });
    JsError::new(&annotated.unwrap_or(msg))
}

impl Engine {
    /// Native-facing constructor: source + frontend -> Engine or error string.
    /// (The wasm-bindgen `new` below is a thin adapter over this.)
    pub fn from_source(source: &str, frontend: &str) -> Result<Engine, String> {
        let fe: Box<dyn doppio::Frontend> = match frontend {
            "hledger" => Box::new(doppio::HledgerFrontend),
            "beancount" => Box::new(doppio::BeancountFrontend),
            _ => Box::new(doppio::LedgerFrontend),
        };
        // No `include` support in the MVP: included files resolve to empty.
        let noop_opener =
            |_path: &str| -> Result<String, Box<dyn std::error::Error>> { Ok(String::new()) };
        let hir = fe
            .parse(source, std::path::Path::new(""), &noop_opener)
            .map_err(|e| e.as_ref().to_string())?;
        let journal = doppio::elaborate(hir, &fe.elaboration_defaults())
            .map_err(|e| e.to_string())?;
        Ok(Engine { journal })
    }

    /// Accumulated amounts per account: `account -> commodity -> total` for
    /// that exact account (not including descendants).
    fn leaf_amounts(&self) -> BTreeMap<String, BTreeMap<String, Decimal>> {
        let mut acc: BTreeMap<String, BTreeMap<String, Decimal>> = BTreeMap::new();
        for txn in &self.journal.transactions {
            for p in &txn.postings {
                if !p.is_real() {
                    continue;
                }
                if let Some(amount) = &p.amount {
                    for (commodity, value) in amount.iter() {
                        *acc.entry(p.account.clone())
                            .or_default()
                            .entry(commodity.to_string())
                            .or_insert(Decimal::ZERO) += value;
                    }
                }
            }
        }
        acc
    }
}

// ---------------------------------------------------------------------------
// Reports
// ---------------------------------------------------------------------------

/// Format one (commodity, value) pair, ledger-style.
fn fmt_amount(commodity: &str, v: Decimal) -> String {
    let s = v.to_string();
    match commodity {
        "$" => format!("${s}"),
        c => format!("{s} {c}"),
    }
}

impl Engine {
    /// Balance report over the elaborated journal.
    pub fn report_balance(&self) -> BalanceReport {
        let leaves = self.leaf_amounts();

        // Fold each leaf account into every ancestor prefix row.
        let mut rows_map: BTreeMap<String, BTreeMap<String, Decimal>> = BTreeMap::new();
        for (account, commodities) in &leaves {
            let mut prefix = String::new();
            for seg in account.split(':') {
                if !prefix.is_empty() {
                    prefix.push(':');
                }
                prefix.push_str(seg);
                let row = rows_map.entry(prefix.clone()).or_default();
                for (c, v) in commodities {
                    *row.entry(c.clone()).or_insert(Decimal::ZERO) += *v;
                }
            }
        }

        // BTreeMap iteration sorts parents before children (shorter prefix
        // first), which is exactly ledger's top-down tree order.
        let rows = rows_map
            .into_iter()
            .map(|(account, amounts)| BalanceRow {
                depth: account.split(':').count() as u32,
                account,
                amounts: amounts
                    .into_iter()
                    .map(|(c, v)| fmt_amount(&c, v))
                    .collect(),
            })
            .collect();

        // Grand total across ALL accounts (ledger's bottom-line sum), per
        // commodity. Journals holding unconverted commodities (AAPL, EUR)
        // legitimately show a non-zero multi-commodity total, like ledger.
        let mut total: BTreeMap<String, Decimal> = BTreeMap::new();
        for commodities in leaves.values() {
            for (c, v) in commodities {
                *total.entry(c.clone()).or_insert(Decimal::ZERO) += *v;
            }
        }

        BalanceReport {
            rows,
            total: total
                .into_iter()
                .map(|(c, v)| fmt_amount(&c, v))
                .collect(),
        }
    }

    /// Register report over the elaborated journal.
    pub fn report_register(&self) -> RegisterReport {
        let mut transactions = Vec::new();
        for txn in &self.journal.transactions {
            // TransactionState proto enum: UNSPECIFIED=0, UNCLEARED=1,
            // PENDING=2, CLEARED=3
            let state = match txn.state {
                3 => "cleared",
                2 => "pending",
                _ => "unmarked",
            };
            let mut postings = Vec::new();
            for p in &txn.postings {
                if !p.is_real() {
                    continue;
                }
                if let Some(amount) = &p.amount {
                    for (commodity, value) in amount.iter() {
                        postings.push(RegisterPosting {
                            account: p.account.clone(),
                            commodity: commodity.to_string(),
                            amount: value.to_string(),
                        });
                    }
                }
            }
            transactions.push(RegisterTxn {
                date: fmt_epoch_days(txn.date),
                state: state.to_string(),
                description: txn.description.clone(),
                postings,
            });
        }
        RegisterReport { transactions }
    }

    /// Sorted account names referenced by postings.
    pub fn report_accounts(&self) -> Vec<String> {
        let mut accounts: Vec<String> = self
            .journal
            .transactions
            .iter()
            .flat_map(|t| t.postings.iter().map(|p| p.account.clone()))
            .collect();
        accounts.sort();
        accounts.dedup();
        accounts
    }
}

/// Days-since-Unix-epoch -> `YYYY-MM-DD` (doppio persists dates as epoch days).
/// 719_163 = `NaiveDate::from_ymd(1970,1,1).num_days_from_ce()`.
fn fmt_epoch_days(days: i32) -> String {
    use chrono::Datelike;
    match chrono::NaiveDate::from_num_days_from_ce_opt(days + 719_163) {
        Some(d) => format!("{:04}-{:02}-{:02}", d.year(), d.month(), d.day()),
        None => format!("<date {days}>"),
    }
}

// ---------------------------------------------------------------------------
// wasm-bindgen surface (constructor throws on parse/elaboration errors)
// ---------------------------------------------------------------------------

#[wasm_bindgen]
impl Engine {
    #[wasm_bindgen(constructor)]
    pub fn new(source: &str, frontend: &str) -> Result<Engine, JsError> {
        console_error_panic_hook::set_once();
        Engine::from_source(source, frontend).map_err(to_js_error)
    }

    /// `ledger bal` equivalent.
    pub fn balance(&self) -> Result<JsValue, JsError> {
        serde_wasm_bindgen::to_value(&self.report_balance())
            .map_err(|e| JsError::new(&e.to_string()))
    }

    /// `ledger reg` equivalent.
    pub fn register(&self) -> Result<JsValue, JsError> {
        serde_wasm_bindgen::to_value(&self.report_register())
            .map_err(|e| JsError::new(&e.to_string()))
    }

    /// `ledger accounts` equivalent.
    pub fn accounts(&self) -> Vec<String> {
        self.report_accounts()
    }
}