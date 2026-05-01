use std::{fs, path::PathBuf};

use zero_engine_client::{
    Brief, ExecuteResponse, ExecuteSide, Positions, RejectionsFeed, Risk, V2Status,
};

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .join("contracts/paper-api")
        .join(name)
}

fn fixture(name: &str) -> String {
    fs::read_to_string(fixture_path(name)).expect("paper API contract fixture should be readable")
}

fn assert_float(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < f64::EPSILON,
        "expected {actual} to equal {expected}"
    );
}

#[test]
fn paper_v2_status_contract_decodes() {
    let status: V2Status =
        serde_json::from_str(&fixture("v2_status.json")).expect("v2 status should decode");

    assert_eq!(
        status.regime(),
        Some("PAPER MARKET. Local deterministic demo.")
    );
    assert_float(status.engine_confidence().expect("confidence score"), 90.0);
    assert_eq!(status.confidence_level(), Some("paper"));
    assert_eq!(status.open(), Some(1));
    assert_float(status.equity().expect("equity"), 10_000.0);
    assert_eq!(status.ts.as_deref(), Some("2026-05-01T00:00:00Z"));
}

#[test]
fn paper_positions_contract_decodes() {
    let positions: Positions =
        serde_json::from_str(&fixture("positions.json")).expect("positions should decode");

    assert_eq!(positions.items.len(), 1);
    assert_eq!(positions.items[0].symbol, "BTC");
    assert_eq!(positions.items[0].side, "long");
    assert_float(positions.items[0].size, 0.01);
    assert_float(positions.account_value.expect("account value"), 10_000.0);
}

#[test]
fn paper_risk_contract_decodes() {
    let risk: Risk = serde_json::from_str(&fixture("risk.json")).expect("risk should decode");

    assert_eq!(risk.open_count, Some(1));
    assert_float(risk.account_value.expect("account value"), 10_000.0);
    assert!(!risk.is_halted());
    assert_eq!(risk.updated_at.as_deref(), Some("2026-05-01T00:00:00Z"));
}

#[test]
fn paper_brief_contract_decodes() {
    let brief: Brief = serde_json::from_str(&fixture("brief.json")).expect("brief should decode");

    assert!(brief.has_content());
    assert_eq!(brief.open_positions, Some(1));
    assert_eq!(brief.positions.len(), 1);
    assert_eq!(brief.last_cycle["mode"], "paper");
    assert_eq!(brief.last_cycle["decisions"], 2);
}

#[test]
fn paper_rejections_contract_decodes() {
    let feed: RejectionsFeed =
        serde_json::from_str(&fixture("rejections.json")).expect("rejections should decode");

    assert_eq!(feed.items.len(), 1);
    assert_eq!(feed.items[0].coin.as_deref(), Some("BTC"));
    assert_eq!(feed.items[0].direction.as_deref(), Some("buy"));
    assert_eq!(feed.items[0].stage.as_deref(), Some("risk"));
    assert_eq!(
        feed.items[0].reason.as_deref(),
        Some("order notional exceeds limit")
    );
}

#[test]
fn paper_execute_contract_decodes() {
    let accepted: ExecuteResponse = serde_json::from_str(&fixture("execute_accepted.json"))
        .expect("accepted execute should decode");
    let rejected: ExecuteResponse = serde_json::from_str(&fixture("execute_rejected.json"))
        .expect("rejected execute should decode");

    assert!(accepted.accepted);
    assert!(accepted.simulated);
    assert_eq!(accepted.fill_id.as_deref(), Some("paper-contract"));
    assert_eq!(accepted.side, Some(ExecuteSide::Buy));
    assert_eq!(accepted.reason.as_deref(), Some("allowed"));

    assert!(!rejected.accepted);
    assert!(rejected.simulated);
    assert_eq!(rejected.fill_id, None);
    assert_eq!(rejected.side, Some(ExecuteSide::Buy));
    assert_eq!(
        rejected.reason.as_deref(),
        Some("order notional exceeds limit")
    );
}
