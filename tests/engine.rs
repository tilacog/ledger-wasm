//! Native tests for the report engine (rlib target), run before the wasm build.
//! Journal expectations cross-checked against ledger-cli 3.4.1 (`ledger bal`).

use ledger_reports_wasm::Engine;

const JOURNAL: &str = r#"
2024-01-01 * Opening balances
    Assets:Checking           $2500.00
    Equity:Opening Balances

2024-01-02 * Paycheck (salary)
    Assets:Checking           $3000.00
    Income:Salary

2024-01-05 Grocery run
    Expenses:Food             $82.40
    Assets:Checking

2024-01-08 * Phone bill
    Expenses:Utilities        $45.00
    Expenses:Internet         $20.00
    Assets:Checking           -$65.00

2024-01-10 * Buy 10 AAPL (cost basis tracked)
    Assets:Brokerage          10 AAPL {$185.00} @ $185.00
    Assets:Checking

2024-01-12 * London trip (currency conversion via @)
    Expenses:Travel           420.00 EUR @ $1.09
    Assets:Checking

2024-01-15 * Sell 5 AAPL (gain over cost booked explicitly)
    Assets:Brokerage          -5 AAPL {$185.00} @ $196.00
    Assets:Checking           $980.00
    Income:Capital Gains      $-55.00

2024-01-20 Balance assertion on running total
    Assets:Checking           $0.00 = $4024.80
    Expenses:Gifts

2024-01-31 * Month-end interest
    Assets:Checking           $12.15
    Income:Interest
"#;

/// Explicitly unbalanced: no null posting to infer, amounts differ.
const UNBALANCED: &str = r#"
2024-02-01 * Broken transaction
    Expenses:Stuff            $10.00
    Assets:Checking           $3.00
"#;

#[test]
fn compiles_demo_journal() {
    let e = Engine::from_source(JOURNAL, "ledger").expect("journal must elaborate");
    let bal = e.report_balance();

    // Assets:Checking leaf: 2500 + 3000 - 82.40 - 65 - 1850 - 457.80
    // + 980 - 1450 (gift) + 12.15 = 4036.95 (matches ledger-cli bal)
    let checking = bal
        .rows
        .iter()
        .find(|r| r.account == "Assets:Checking")
        .expect("Checking row");
    assert_eq!(
        checking.amounts,
        vec!["$4036.95".to_string()],
        "checking leaf: {:?}",
        checking.amounts
    );

    // Brokerage nets to 5 AAPL (buy 10, sell 5)
    let brokerage = bal
        .rows
        .iter()
        .find(|r| r.account == "Assets:Brokerage")
        .expect("Brokerage row");
    assert_eq!(brokerage.amounts, vec!["5 AAPL".to_string()]);

    // Tree folding: the Assets parent row carries both commodities
    let assets = bal
        .rows
        .iter()
        .find(|r| r.account == "Assets")
        .expect("Assets row");
    assert!(
        assets.amounts.contains(&"$4036.95".to_string()),
        "Assets row: {:?}",
        assets.amounts
    );
    assert!(assets.amounts.contains(&"5 AAPL".to_string()));

    // register: 9 transactions; first is the opening one
    let reg = e.report_register();
    assert_eq!(reg.transactions.len(), 9);
    assert_eq!(reg.transactions[0].date, "2024-01-01");
    assert_eq!(reg.transactions[0].state, "cleared");
    assert_eq!(reg.transactions[0].description, "Opening balances");

    // total row — matches ledger-cli bal's bottom line: $-1382.80,
    // 5 AAPL, 420.00 EUR (unconverted commodities make it non-zero)
    let bal = e.report_balance();
    assert_eq!(
        bal.total,
        vec!["$-1382.80".to_string(), "5 AAPL".to_string(), "420.00 EUR".to_string()],
        "total: {:?}",
        bal.total
    );

    // accounts list, sorted: exactly the 11 referenced accounts
    let accounts = e.report_accounts();
    assert_eq!(
        accounts,
        vec![
            "Assets:Brokerage",
            "Assets:Checking",
            "Equity:Opening Balances",
            "Expenses:Food",
            "Expenses:Gifts",
            "Expenses:Internet",
            "Expenses:Travel",
            "Expenses:Utilities",
            "Income:Capital Gains",
            "Income:Interest",
            "Income:Salary",
        ]
    );
}

#[test]
fn unbalanced_journal_is_rejected() {
    let msg = Engine::from_source(UNBALANCED, "ledger")
        .err()
        .expect("unbalanced txn must fail elaboration");
    assert!(
        msg.to_lowercase().contains("balance"),
        "error should mention balance: {msg}"
    );
}

#[test]
fn hledger_frontend_works() {
    let src = r#"
2024-01-01 Opening
    assets:cash   $100
    equity
"#;
    let e = Engine::from_source(src, "hledger").expect("hledger parse");
    let bal = e.report_balance();
    let cash = bal
        .rows
        .iter()
        .find(|r| r.account == "assets:cash")
        .expect("row");
    assert_eq!(cash.amounts, vec!["$100".to_string()]);
}

#[test]
fn parse_error_is_surfaced() {
    // empty journal is fine
    assert!(Engine::from_source("", "ledger").is_ok());
    // malformed: commodity sign with no number
    let bad = "2024-01-01 x\n    A  $\n    B\n";
    let err = Engine::from_source(bad, "ledger")
        .err()
        .expect("malformed amount must fail");
    assert!(
        err.contains("-->") || err.contains("expected"),
        "error should reference the syntax problem: {err}"
    );
}

#[test]
fn decimal_math_is_exact() {
    // The reason the engine is Rust/wasm and not JS floats:
    // 2651.95 - 1.95 must be exactly 2650, no 0.30000000000000004-style drift.
    let a = rust_decimal::Decimal::new(265195, 2);
    assert_eq!(
        (a - rust_decimal::Decimal::new(195, 2))
            .normalize()
            .to_string(),
        "2650"
    );
}