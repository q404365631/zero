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
