//! Breadth tests for the HTTP surface.
//!
//! Covers every read endpoint the CLI calls. The mock is kept in
//! lockstep with the real engine's wire format — see the captured
//! fixtures in `tests/fixtures/`. These tests assert the typed
//! shape round-trips cleanly and the fields the CLI actually
//! renders carry the expected values.
//!
//! Regression history: an earlier version of the mock invented
//! `headline` / `gates` / `kill_all` / flat `engine_confidence`
//! fields that the live engine never emits. Tests passed, the
//! real CLI rendered em-dashes. Do not let that happen again —
//! if you change a struct, update the fixture *and* prove it
//! against the live engine, not just against the mock.

use zero_engine_client::HttpClient;
use zero_testkit::mock_engine::MockEngine;

async fn client() -> (MockEngine, HttpClient) {
    let mock = MockEngine::spawn().await.expect("spawn mock");
    let http = HttpClient::new(mock.base_url(), None).expect("client");
    (mock, http)
}

#[tokio::test]
async fn v2_status_decodes_nested_shape() {
    let (mock, http) = client().await;
    let s = http.v2_status().await.expect("v2_status");
    // Accessors read through to the nested sub-objects. The
    // live engine publishes `confidence.score` as 0..=100, not
    // 0..=1; regime lives under `market.regime`.
    assert_eq!(s.regime(), Some("TREND_LONG confirmed across majors."));
    assert_eq!(s.open(), Some(2));
    assert_eq!(s.engine_confidence(), Some(72.0));
    assert_eq!(s.confidence_level(), Some("high"));
    assert_eq!(s.equity(), Some(10_034.12));
    assert_eq!(s.today.trades, Some(24));
    assert_eq!(s.market.fear_greed, Some(54));
    mock.shutdown().await;
}

#[tokio::test]
async fn positions_decodes_two() {
    let (mock, http) = client().await;
    let p = http.positions().await.expect("positions");
    assert_eq!(p.items.len(), 2);
    assert_eq!(p.items[0].symbol, "BTC");
    assert_eq!(p.items[0].side, "long");
    assert!((p.items[0].size - 0.42).abs() < 1e-9);
    assert_eq!(p.items[1].symbol, "ETH");
    assert_eq!(p.items[1].side, "short");
    assert_eq!(p.account_value, Some(10_034.12));
    mock.shutdown().await;
}

#[tokio::test]
async fn risk_decodes_and_is_not_halted() {
    let (mock, http) = client().await;
    let r = http.risk().await.expect("risk");
    assert!(!r.is_halted());
    assert_eq!(r.open_count, Some(2));
    assert_eq!(r.account_value, Some(10_034.12));
    assert_eq!(r.daily_pnl_usd, Some(34.12));
    // Derived percent: daily_loss_usd / peak_equity * 100
    // = 4.1261 / 10_100 * 100 ≈ 0.0408529...
    let pct = r.daily_loss_pct().expect("derived daily-loss pct");
    assert!((pct - 0.040_852_475).abs() < 1e-6, "got {pct}");
    mock.shutdown().await;
}

#[tokio::test]
async fn hyperliquid_status_decodes_read_only_shape() {
    let (mock, http) = client().await;
    let status = http
        .hyperliquid_status(Some("BTC"))
        .await
        .expect("hl-status");

    assert!(status.enabled);
    assert_eq!(status.exchange.as_deref(), Some("hyperliquid"));
    assert_eq!(status.secrets_required, Some(false));
    assert_eq!(status.mids.get("BTC"), Some(&40500.0));
    mock.shutdown().await;
}

#[tokio::test]
async fn market_quote_decodes_active_quote_source() {
    let (mock, http) = client().await;
    let quote = http.market_quote("BTC").await.expect("market quote");

    assert_eq!(quote.symbol, "BTC");
    assert!((quote.price - 40500.0).abs() < 1e-9);
    assert_eq!(quote.source, "paper:static");
    assert!(!quote.live);
    mock.shutdown().await;
}

#[tokio::test]
async fn live_certification_decodes_dry_run_harness() {
    let (mock, http) = client().await;
    let report = http.live_certification().await.expect("live certification");

    assert_eq!(report.schema_version, "zero.live_certification.v1");
    assert!(report.passed);
    assert!(report.live_start_certified);
    assert_eq!(report.drills.len(), 2);
    assert_eq!(report.evidence_requirements[0], "live_preflight packet");
    mock.shutdown().await;
}

#[tokio::test]
async fn live_cockpit_decodes_operator_readiness_packet() {
    let (mock, http) = client().await;
    let cockpit = http.live_cockpit().await.expect("live cockpit");

    assert_eq!(cockpit.schema_version, "zero.live_cockpit.v1");
    assert_eq!(cockpit.live_mode, "refused");
    assert!(!cockpit.ready);
    assert!(!cockpit.risk_increasing_allowed);
    assert_eq!(cockpit.preflight.failed_checks[0].name, "live_executor");
    assert_eq!(cockpit.immune.open_breakers[0].name, "dead_man");
    assert!(cockpit.certification.passed);
    assert!(cockpit.heartbeat.expired);
    assert_eq!(cockpit.live_records.total, 0);
    assert_eq!(cockpit.operator_context.handle, "mock-operator");
    assert_eq!(cockpit.operator_context.scope, "local-private");
    assert!(cockpit.next_action.contains("live_executor"));
    mock.shutdown().await;
}

#[tokio::test]
async fn live_evidence_decodes_hash_only_canary_bundle() {
    let (mock, http) = client().await;
    let evidence = http.live_evidence().await.expect("live evidence");

    assert_eq!(evidence.schema_version, "zero.live_evidence.v1");
    assert_eq!(evidence.live_mode, "refused");
    assert!(!evidence.ready);
    assert!(!evidence.risk_increasing_allowed);
    assert_eq!(evidence.operator_context.handle, "mock-operator");
    assert_eq!(evidence.artifacts.len(), 9);
    assert_eq!(evidence.artifacts[0].included, "hash_only");
    assert!(evidence.artifacts[0].hash.starts_with("sha256:"));
    assert!(
        evidence
            .artifacts
            .iter()
            .any(|artifact| artifact.name == "live_execution_receipts"
                && artifact.schema_version == "zero.live_execution_receipts.v1")
    );
    assert!(evidence.evidence_hash.starts_with("sha256:"));
    assert_eq!(
        evidence
            .signature
            .get("status")
            .and_then(serde_json::Value::as_str),
        Some("unsigned_local")
    );
    mock.shutdown().await;
}

#[tokio::test]
async fn live_canary_policy_decodes_public_claim_boundary() {
    let (mock, http) = client().await;
    let policy = http.live_canary_policy().await.expect("live canary policy");

    assert_eq!(policy.schema_version, "zero.live_canary_policy.v1");
    assert_eq!(policy.policy_version, "zero.live_canary_policy.public.v1");
    assert_eq!(policy.mode, "refusal");
    assert!(!policy.summary.ready_for_canary);
    assert!(!policy.summary.policy_armed);
    assert!(policy.summary.qualified);
    assert!(policy.summary.refusal_evidence_qualified);
    assert!(!policy.summary.publishable_canary_evidence);
    assert!(!policy.summary.live_order_accepted);
    assert_eq!(policy.summary.receipts_accepted, 0);
    assert_eq!(
        policy.recommendation.action,
        "keep_public_claim_at_refusal_proof"
    );
    assert_eq!(policy.recommendation.risk_direction, "none");
    assert_eq!(policy.operator_context.handle, "mock-operator");
    assert!(policy.request.is_some());
    assert!(
        policy
            .phases
            .iter()
            .any(|phase| phase.name == "qualification" && phase.status == "pass")
    );
    mock.shutdown().await;
}

#[test]
fn live_canary_policy_decodes_null_request_from_runtime_readiness() {
    let policy: zero_engine_client::LiveCanaryPolicy = serde_json::from_value(serde_json::json!({
        "schema_version": "zero.live_canary_policy.v1",
        "policy_version": "zero.live_canary_policy.public.v1",
        "generated_at": "2026-05-03T22:59:16Z",
        "mode": "runtime-readiness",
        "summary": {
            "ready_for_canary": false,
            "policy_armed": false,
            "live_order_attempted": false,
            "live_order_accepted": false,
            "receipts_accepted": 0,
            "exchange_evidence_attached": false,
            "publishable_canary_evidence": false,
            "refusal_evidence_qualified": false,
            "qualified": false,
            "next_step": "fix_live_preflight_before_canary"
        },
        "policy": {},
        "phases": [],
        "recommendation": {
            "action": "fix_live_preflight_before_canary",
            "risk_direction": "down",
            "reason": "preflight is not ready"
        },
        "operator_context": {
            "schema_version": "zero.operator_context.v1",
            "handle": "local-operator",
            "operator_id": "local-operator",
            "role": "owner",
            "scope": "local-private",
            "source": "runtime-default"
        },
        "request": null,
        "privacy": {}
    }))
    .expect("runtime readiness canary policy with null request decodes");

    assert_eq!(policy.mode, "runtime-readiness");
    assert!(policy.request.is_none());
    assert_eq!(
        policy.recommendation.action,
        "fix_live_preflight_before_canary"
    );
}

#[tokio::test]
async fn live_receipts_decode_public_safe_receipt_bundle() {
    let (mock, http) = client().await;
    let receipts = http.live_receipts().await.expect("live receipts");

    assert_eq!(receipts.schema_version, "zero.live_execution_receipts.v1");
    assert_eq!(receipts.operator_context.handle, "mock-operator");
    assert!(receipts.receipts.is_empty());
    assert!(receipts.receipts_hash.starts_with("sha256:"));
    assert_eq!(
        receipts
            .summary
            .get("status")
            .and_then(serde_json::Value::as_str),
        Some("empty")
    );
    mock.shutdown().await;
}

#[tokio::test]
async fn runtime_parity_decodes_production_ooda_boundary() {
    let (mock, http) = client().await;
    let report = http.runtime_parity().await.expect("runtime parity");

    assert_eq!(report.schema_version, "zero.runtime.production_parity.v1");
    assert!(report.ok);
    assert!(report.paper_only);
    assert!(!report.places_live_orders);
    assert_eq!(report.paper.decisions, 4);
    assert_eq!(report.paper.fills, 2);
    assert_eq!(report.paper.rejections, 2);
    assert_eq!(report.live_shadow.mode, "disabled-fail-closed");
    assert_eq!(report.live_shadow.refused, 4);
    assert_eq!(report.live_shadow.accepted, 0);
    assert!((report.feedback.rejection_rate - 0.5).abs() < f64::EPSILON);
    assert_eq!(
        report
            .claim_boundary
            .get("live_trading_claimed")
            .and_then(serde_json::Value::as_bool),
        Some(false)
    );
    mock.shutdown().await;
}

#[tokio::test]
async fn immune_decodes_risk_blocking_breakers() {
    let (mock, http) = client().await;
    let report = http.immune().await.expect("immune");

    assert_eq!(report.schema_version, "zero.immune.v1");
    assert!(!report.risk_increasing_allowed);
    assert_eq!(report.breakers.len(), 3);
    assert!(
        report
            .breakers
            .iter()
            .any(|b| b.name == "dead_man" && b.blocks_risk)
    );
    mock.shutdown().await;
}

#[tokio::test]
async fn regime_decodes_without_coin() {
    let (mock, http) = client().await;
    let r = http.regime(None).await.expect("regime");
    assert_eq!(r.regime.as_deref(), Some("TREND_LONG"));
    assert!((r.confidence.unwrap() - 0.81).abs() < 1e-6);
    mock.shutdown().await;
}

#[tokio::test]
async fn regime_decodes_with_coin_query() {
    let (mock, http) = client().await;
    // Mock ignores the query but the client must still encode it;
    // this exercises the urlencoding path without a server assertion.
    let r = http.regime(Some("BTC")).await.expect("regime-btc");
    assert_eq!(r.regime.as_deref(), Some("TREND_LONG"));
    mock.shutdown().await;
}

#[tokio::test]
async fn brief_decodes_real_shape() {
    let (mock, http) = client().await;
    let b = http.brief().await.expect("brief");
    assert_eq!(b.fear_greed, Some(54));
    assert_eq!(b.open_positions, Some(2));
    assert_eq!(b.positions.len(), 1);
    assert_eq!(b.positions[0].symbol, "BTC");
    assert!(!b.recent_signals.is_empty());
    assert!(b.has_content(), "populated brief must report content");
    mock.shutdown().await;
}

#[tokio::test]
async fn evaluate_decodes_layers_and_derives_verdict() {
    let (mock, http) = client().await;
    let e = http.evaluate("SOL").await.expect("evaluate");
    assert_eq!(e.coin.as_deref(), Some("SOL"));
    assert_eq!(e.direction.as_deref(), Some("NONE"));
    assert_eq!(e.layers.len(), 3);
    assert_eq!(e.layers[0].layer, "layer_0");
    assert!(e.layers[0].passed);
    // One failed layer in the fixture → REJECT derivation.
    assert_eq!(e.verdict(), "REJECT");
    mock.shutdown().await;
}

#[tokio::test]
async fn pulse_decodes_with_clamp() {
    let (mock, http) = client().await;
    // request an out-of-range limit; client should clamp and not 400.
    let p = http.pulse(10_000).await.expect("pulse");
    assert_eq!(p.items.len(), 2);
    assert_eq!(p.items[0].kind.as_deref(), Some("signal"));
    mock.shutdown().await;
}

#[tokio::test]
async fn approaching_decodes() {
    let (mock, http) = client().await;
    let a = http.approaching().await.expect("approaching");
    assert_eq!(a.items.len(), 2);
    assert_eq!(a.items[0].coin, "AVAX");
    assert_eq!(a.items[0].gate.as_deref(), Some("edge_floor"));
    mock.shutdown().await;
}

#[tokio::test]
async fn rejections_decodes_with_filter() {
    let (mock, http) = client().await;
    let r = http.rejections(50, Some("SOL")).await.expect("rejections");
    assert_eq!(r.items.len(), 1);
    assert_eq!(r.items[0].coin.as_deref(), Some("SOL"));
    mock.shutdown().await;
}

#[tokio::test]
async fn rejections_decodes_no_filter() {
    let (mock, http) = client().await;
    let r = http.rejections(20, None).await.expect("rejections-all");
    assert_eq!(r.items.len(), 1);
    mock.shutdown().await;
}
