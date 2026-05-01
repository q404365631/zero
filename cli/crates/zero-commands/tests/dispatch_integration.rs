//! End-to-end dispatcher tests against the `zero-testkit` mock
//! engine. Verifies that every HTTP-backed command pulls real
//! data, decodes it, and emits the right `OutputLine` kind.

use std::sync::{Arc, Mutex};

use zero_commands::config::MockConfig;
use zero_commands::{
    ConfigDoctorFinding, DispatchContext, ModeTarget, OutputLine, OverlayTarget, ReplayEvent,
    ReplayKind, RiskDirection, SessionError, SessionSource, SessionSummary, dispatch,
};
use zero_engine_client::{EngineState, HttpClient};
use zero_testkit::mock_engine::MockEngine;

async fn ctx_with_mock() -> (MockEngine, DispatchContext) {
    let mock = MockEngine::spawn().await.expect("spawn mock");
    let client = HttpClient::new(mock.base_url(), None).expect("client");
    let ctx = DispatchContext::new(Some(client), EngineState::shared());
    (mock, ctx)
}

#[tokio::test]
async fn status_renders_engine_summary() {
    let (mock, ctx) = ctx_with_mock().await;
    let out = dispatch(&ctx, "/status").await.unwrap().unwrap();
    assert_eq!(out.risk, Some(RiskDirection::Neutral));
    let line = &out.lines[0];
    let OutputLine::Command(s) = line else {
        panic!("expected Command line, got {line:?}");
    };
    assert!(s.contains("regime="));
    assert!(s.contains("equity="));
    mock.shutdown().await;
}

#[tokio::test]
async fn brief_emits_headline_or_system() {
    let (mock, ctx) = ctx_with_mock().await;
    let out = dispatch(&ctx, "/brief").await.unwrap().unwrap();
    assert!(!out.lines.is_empty());
    mock.shutdown().await;
}

#[tokio::test]
async fn risk_command_decodes_summary() {
    let (mock, ctx) = ctx_with_mock().await;
    let out = dispatch(&ctx, "/risk").await.unwrap().unwrap();
    let OutputLine::Command(s) = &out.lines[0] else {
        panic!("expected Command, got {:?}", out.lines);
    };
    // Field vocabulary matches the live engine's wire shape.
    // The legacy `exposure=` column was removed — the real
    // engine does not emit an exposure percent, only raw dollar
    // amounts plus drawdown/peak/equity.
    assert!(s.contains("risk: OK"), "state prefix: {s}");
    assert!(s.contains("equity="), "equity field: {s}");
    assert!(s.contains("peak="), "peak-equity field: {s}");
    assert!(s.contains("dd="), "drawdown field: {s}");
    assert!(s.contains("daily-pnl="), "daily-pnl field: {s}");
    assert!(s.contains("daily-loss="), "daily-loss field: {s}");
    assert!(s.contains("open="), "open-count field: {s}");
    mock.shutdown().await;
}

#[tokio::test]
async fn hl_status_renders_read_only_exchange_status() {
    let (mock, ctx) = ctx_with_mock().await;
    let out = dispatch(&ctx, "/hl-status BTC").await.unwrap().unwrap();

    assert_eq!(out.risk, Some(RiskDirection::Neutral));
    let OutputLine::Command(s) = &out.lines[0] else {
        panic!("expected Command, got {:?}", out.lines);
    };
    assert!(s.contains("hl: enabled"), "enabled row: {s}");
    assert!(s.contains("secrets_required=false"), "secrets field: {s}");
    assert!(
        out.lines
            .iter()
            .any(|line| matches!(line, OutputLine::System(s) if s.contains("BTC"))),
        "BTC mid row missing: {:?}",
        out.lines
    );
    mock.shutdown().await;
}

#[tokio::test]
async fn quote_renders_active_quote_source() {
    let (mock, ctx) = ctx_with_mock().await;
    let out = dispatch(&ctx, "/quote BTC").await.unwrap().unwrap();

    assert_eq!(out.risk, Some(RiskDirection::Neutral));
    let OutputLine::Command(s) = &out.lines[0] else {
        panic!("expected Command, got {:?}", out.lines);
    };
    assert!(s.contains("quote BTC:"), "quote prefix: {s}");
    assert!(s.contains("40500.0000"), "price: {s}");
    assert!(s.contains("source=paper:static"), "source: {s}");
    mock.shutdown().await;
}

#[tokio::test]
async fn quote_without_coin_emits_usage_hint() {
    let (mock, ctx) = ctx_with_mock().await;
    let out = dispatch(&ctx, "/quote").await.unwrap().unwrap();

    assert_eq!(out.risk, Some(RiskDirection::Neutral));
    assert!(
        matches!(&out.lines[0], OutputLine::Warn(s) if s.contains("/quote <coin>")),
        "usage line: {:?}",
        out.lines
    );
    mock.shutdown().await;
}

#[tokio::test]
async fn risk_flags_equity_above_peak_inconsistency() {
    // Production drift: the engine wrote `risk.json` with
    // account_value=638 and peak_equity=577. Equity above peak is
    // impossible by definition — peak is the monotonic max of
    // equity. The dispatcher must surface the contradiction via
    // a warn line AND suppress the drawdown percent (computed
    // against a stale peak, so it's fiction).
    let (mock, ctx) = ctx_with_mock().await;
    mock.with_overrides(|o| o.force_stale_risk_equity = true);
    let out = dispatch(&ctx, "/risk").await.unwrap().unwrap();

    // Primary line still renders — the dollar fields are useful
    // even when cross-consistency is off.
    let OutputLine::Command(primary) = &out.lines[0] else {
        panic!("expected Command line first, got {:?}", out.lines);
    };
    assert!(
        primary.contains("equity=$638.49"),
        "equity still rendered: {primary}"
    );
    assert!(
        primary.contains("peak=$577.34"),
        "peak still rendered: {primary}"
    );
    // Drawdown must NOT display a confident percent — it was
    // computed against a stale peak.
    assert!(
        primary.contains("dd=—"),
        "dd must be suppressed when equity > peak: {primary}"
    );
    assert!(
        !primary.contains("dd=0.22%"),
        "stale dd percent must not leak through: {primary}"
    );

    // Warn line explains the contradiction.
    let warn = out
        .lines
        .iter()
        .find_map(|l| match l {
            OutputLine::Warn(s) => Some(s.clone()),
            _ => None,
        })
        .expect("warn line present");
    assert!(
        warn.to_lowercase().contains("inconsistent"),
        "warn flags the inconsistency: {warn}"
    );
    assert!(
        warn.contains("equity > peak"),
        "warn names the contradiction: {warn}"
    );
    mock.shutdown().await;
}

#[tokio::test]
async fn regime_without_coin_uses_market_label() {
    let (mock, ctx) = ctx_with_mock().await;
    let out = dispatch(&ctx, "/regime").await.unwrap().unwrap();
    let OutputLine::Command(s) = &out.lines[0] else {
        panic!("expected Command, got {:?}", out.lines);
    };
    assert!(s.starts_with("regime[market]"));
    mock.shutdown().await;
}

#[tokio::test]
async fn regime_with_coin_uses_coin_label() {
    let (mock, ctx) = ctx_with_mock().await;
    let out = dispatch(&ctx, "/regime SOL").await.unwrap().unwrap();
    let OutputLine::Command(s) = &out.lines[0] else {
        panic!("expected Command, got {:?}", out.lines);
    };
    assert!(s.contains("[SOL]"));
    mock.shutdown().await;
}

#[tokio::test]
async fn regime_empty_body_alerts_instead_of_emdashes() {
    // Production engines sometimes expose `/regime` but return
    // bare `{}` — the CLI used to render that as a row of
    // em-dashes (`regime[market]: — confidence=—`) that looked
    // like legitimate data. The dispatcher must surface it as
    // an alert so the operator knows the engine has no reading.
    let (mock, ctx) = ctx_with_mock().await;
    mock.with_overrides(|o| o.force_empty_regime = true);
    let out = dispatch(&ctx, "/regime").await.unwrap().unwrap();
    let OutputLine::Alert(s) = &out.lines[0] else {
        panic!("expected Alert on empty regime, got {:?}", out.lines);
    };
    assert!(s.contains("regime[market]"), "alert scopes the coin: {s}");
    assert!(
        s.to_lowercase().contains("empty") || s.to_lowercase().contains("no regime"),
        "alert explains the empty body: {s}"
    );
    // Must NOT render em-dashes — the whole point of the fix is
    // to stop pretending an empty response is data.
    assert!(!s.contains("—"), "must not render em-dash data: {s}");
    mock.shutdown().await;
}

#[tokio::test]
async fn regime_error_envelope_surfaces_engine_message() {
    // Some engine paths return `{"error": "coin not found"}` at
    // HTTP 200. The dispatcher must lift that message up as an
    // alert instead of silently decoding to em-dashes.
    let (mock, ctx) = ctx_with_mock().await;
    mock.with_overrides(|o| o.force_regime_error_envelope = true);
    let out = dispatch(&ctx, "/regime FAKE").await.unwrap().unwrap();
    let OutputLine::Alert(s) = &out.lines[0] else {
        panic!("expected Alert on error envelope, got {:?}", out.lines);
    };
    assert!(s.contains("[FAKE]"), "alert scopes the coin: {s}");
    assert!(
        s.contains("coin not found"),
        "alert echoes the engine message verbatim: {s}"
    );
    mock.shutdown().await;
}

#[tokio::test]
async fn positions_lists_items() {
    let (mock, ctx) = ctx_with_mock().await;
    let out = dispatch(&ctx, "/pos").await.unwrap().unwrap();
    assert!(!out.lines.is_empty());
    mock.shutdown().await;
}

#[tokio::test]
async fn mode_switch_emits_target() {
    // `/heat` is the inline heat readout; the mode switch is
    // reached via the explicit `-mode` synonym so typing `/heat`
    // answers "how hot am I?" without leaving the current pane.
    let (mock, ctx) = ctx_with_mock().await;
    let out = dispatch(&ctx, "/heat-mode").await.unwrap().unwrap();
    assert_eq!(out.mode_change, Some(ModeTarget::Heat));
    assert_eq!(out.risk, Some(RiskDirection::Neutral));
    mock.shutdown().await;
}

#[tokio::test]
async fn heat_readout_summarizes_risk() {
    // Default mock posture is healthy (no kill, no breaker) so
    // the line should land as a neutral Command, include every
    // field, and never claim CRITICAL.
    let (mock, ctx) = ctx_with_mock().await;
    let out = dispatch(&ctx, "/heat").await.unwrap().unwrap();
    assert_eq!(out.risk, Some(RiskDirection::Neutral));
    assert_eq!(out.mode_change, None, "inline command must not mode-switch");
    assert_eq!(out.lines.len(), 1, "heat emits exactly one line");
    let OutputLine::Command(s) = &out.lines[0] else {
        panic!("expected Command, got {:?}", out.lines);
    };
    // Heat vocabulary tracks the real-shape risk fields. `pos=` /
    // `kill=` / `breaker=` / `exposure=` were retired along with
    // the mock fields that fed them; the engine publishes
    // `halted` + `capital_floor_hit` + `open_count` instead.
    assert!(s.starts_with("heat: "), "line prefix is stable: {s}");
    assert!(s.contains("dd="), "drawdown field present: {s}");
    assert!(s.contains("daily-loss="), "daily-loss field present: {s}");
    assert!(s.contains("open="), "open-count field present: {s}");
    assert!(s.contains("halted="), "halted flag present: {s}");
    assert!(s.contains("floor="), "capital-floor flag present: {s}");
    assert!(
        !s.contains("CRITICAL"),
        "healthy mock should not report CRITICAL: {s}"
    );
    mock.shutdown().await;
}

#[tokio::test]
async fn heat_surfaces_alert_when_engine_rejects() {
    // HTTP failures on a health-critical readout must surface as
    // an Alert, not a silent Command — the operator relies on
    // `/heat` to tell them the truth about their guardrails.
    let (mock, ctx) = ctx_with_mock().await;
    mock.with_overrides(|o| o.force_server_error = true);
    let out = dispatch(&ctx, "/heat").await.unwrap().unwrap();
    assert_eq!(out.lines.len(), 1);
    assert!(matches!(out.lines[0], OutputLine::Alert(_)));
    let OutputLine::Alert(s) = &out.lines[0] else {
        unreachable!();
    };
    assert!(s.starts_with("heat: "), "alert prefix matches: {s}");
    mock.shutdown().await;
}

#[tokio::test]
async fn heat_without_http_alerts_operator() {
    // No HTTP client attached (common in tests, --no-persist
    // boot paths, etc.) must produce a single alert — not a
    // panic or an empty line.
    let ctx = DispatchContext::new(None, EngineState::shared());
    let out = dispatch(&ctx, "/heat").await.unwrap().unwrap();
    assert_eq!(out.lines.len(), 1);
    assert!(matches!(out.lines[0], OutputLine::Alert(_)));
}

#[tokio::test]
async fn quit_is_risk_reducer_and_flags_quit() {
    let (mock, ctx) = ctx_with_mock().await;
    let out = dispatch(&ctx, "/quit").await.unwrap().unwrap();
    assert!(out.quit);
    assert_eq!(out.risk, Some(RiskDirection::Reduces));
    mock.shutdown().await;
}

#[tokio::test]
async fn evaluate_opens_verdict_overlay_with_engine_payload() {
    let (mock, ctx) = ctx_with_mock().await;
    let out = dispatch(&ctx, "/evaluate BTC").await.unwrap().unwrap();
    // Neutral command — must not be gated by friction.
    assert_eq!(out.risk, Some(RiskDirection::Neutral));
    // The overlay is the output surface; no extra lines.
    assert!(
        out.lines.is_empty(),
        "verdict dispatch must not emit duplicate text lines: {:?}",
        out.lines
    );
    match out.show_overlay {
        Some(OverlayTarget::Verdict(eval)) => {
            assert_eq!(eval.coin.as_deref(), Some("BTC"));
            // One layer fails in the mock fixture, so the derived
            // verdict is REJECT; the overlay must carry the real
            // per-layer payload instead of a synthesized gate map.
            assert_eq!(eval.verdict(), "REJECT");
            assert_eq!(eval.layers.len(), 3);
            assert_eq!(eval.layers[0].layer, "layer_0");
            assert_eq!(eval.direction.as_deref(), Some("NONE"));
            assert!(eval.conviction.is_some());
        }
        other => panic!("expected Verdict overlay, got {other:?}"),
    }
    mock.shutdown().await;
}

#[tokio::test]
async fn evaluate_without_coin_emits_usage_hint() {
    let (mock, ctx) = ctx_with_mock().await;
    let out = dispatch(&ctx, "/evaluate").await.unwrap().unwrap();
    assert!(
        out.show_overlay.is_none(),
        "missing coin must NOT open an overlay"
    );
    assert_eq!(out.lines.len(), 1);
    let OutputLine::Warn(s) = &out.lines[0] else {
        panic!("expected Warn usage hint, got {:?}", out.lines);
    };
    assert!(s.contains("/evaluate"), "hint mentions command: {s}");
    assert!(s.contains("coin"), "hint explains missing arg: {s}");
    mock.shutdown().await;
}

#[tokio::test]
async fn evaluate_http_404_surfaces_alert_without_overlay() {
    let (mock, ctx) = ctx_with_mock().await;
    mock.with_overrides(|o| o.force_not_found = true);
    let out = dispatch(&ctx, "/evaluate BTC").await.unwrap().unwrap();
    assert!(
        out.show_overlay.is_none(),
        "404 must NOT open an overlay — the operator deserves the error"
    );
    // HTTP failures must also tear down any prior (stale) overlay
    // so the alert is visible instead of hidden behind an older
    // verdict card pinned on a different coin.
    assert!(
        out.dismiss_overlay,
        "HTTP failure on /evaluate must dismiss stale overlays"
    );
    assert_eq!(out.lines.len(), 1);
    let OutputLine::Alert(s) = &out.lines[0] else {
        panic!("expected Alert on 404, got {:?}", out.lines);
    };
    assert!(
        s.to_lowercase().contains("evaluate"),
        "alert mentions command: {s}"
    );
    assert!(s.contains("BTC"), "alert mentions coin: {s}");
    mock.shutdown().await;
}

#[tokio::test]
async fn evaluate_http_500_surfaces_alert_without_overlay() {
    let (mock, ctx) = ctx_with_mock().await;
    mock.with_overrides(|o| o.force_server_error = true);
    let out = dispatch(&ctx, "/evaluate ETH").await.unwrap().unwrap();
    assert!(out.show_overlay.is_none(), "500 must NOT open an overlay");
    assert!(
        out.dismiss_overlay,
        "HTTP 500 on /evaluate must dismiss stale overlays"
    );
    assert!(
        matches!(out.lines.first(), Some(OutputLine::Alert(_))),
        "expected Alert, got {:?}",
        out.lines
    );
    mock.shutdown().await;
}

#[tokio::test]
async fn evaluate_empty_response_emits_alert_and_dismisses_overlay() {
    // Guard against the degenerate-200 failure mode that stranded
    // an operator in the screenshot: engine returns HTTP 200 but
    // the body has no layers and no direction. Rendering that as
    // a verdict card shows the "no verdict — `/evaluate <coin>`
    // to request one" placeholder, which makes it look like the
    // request silently failed. The dispatcher must emit a real
    // alert AND dismiss any stale overlay instead.
    let (mock, ctx) = ctx_with_mock().await;
    mock.with_overrides(|o| o.force_empty_evaluation = true);
    let out = dispatch(&ctx, "/evaluate BTC").await.unwrap().unwrap();
    assert!(
        out.show_overlay.is_none(),
        "empty 200 must NOT open an overlay: {:?}",
        out.show_overlay
    );
    assert!(out.dismiss_overlay, "empty 200 must dismiss stale overlays");
    let OutputLine::Alert(s) = out.lines.first().expect("alert line") else {
        panic!("expected Alert on empty evaluate, got {:?}", out.lines);
    };
    assert!(
        s.to_lowercase().contains("empty"),
        "alert explains why: {s}"
    );
    assert!(s.contains("BTC"), "alert mentions coin: {s}");
    mock.shutdown().await;
}

#[tokio::test]
async fn evaluate_warns_on_extra_args_and_still_runs() {
    // `/evaluate sol short` — the operator thinks they can bias
    // direction. The engine endpoint does not take a direction
    // input, so the extra must surface as a warning, but the
    // evaluate itself must still run (the coin is unambiguous).
    let (mock, ctx) = ctx_with_mock().await;
    let out = dispatch(&ctx, "/evaluate BTC short now")
        .await
        .unwrap()
        .unwrap();
    // Overlay still opened against the real mock payload.
    assert!(
        matches!(out.show_overlay, Some(OverlayTarget::Verdict(_))),
        "evaluate must still run despite the extras: {:?}",
        out.show_overlay
    );
    // Exactly one warning line, echoing the ignored tokens.
    let warns: Vec<_> = out
        .lines
        .iter()
        .filter_map(|l| match l {
            OutputLine::Warn(s) => Some(s.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(warns.len(), 1, "exactly one warn line: {:?}", out.lines);
    let w = &warns[0];
    assert!(w.contains("short"), "warn echoes ignored token: {w}");
    assert!(w.contains("now"), "warn echoes all ignored tokens: {w}");
    mock.shutdown().await;
}

#[tokio::test]
async fn clear_dismisses_stale_overlay() {
    // `/clear` is the operator's clean-slate affordance. Part of
    // clearing the conversation is dismissing any modal overlay
    // that would otherwise keep obscuring the next command's
    // output. This test pins the contract at the dispatch layer;
    // the TUI-side wiring has its own test in zero-tui.
    let (mock, ctx) = ctx_with_mock().await;
    let out = dispatch(&ctx, "/clear").await.unwrap().unwrap();
    assert!(out.clear_log, "/clear must clear the log");
    assert!(
        out.dismiss_overlay,
        "/clear must also dismiss any stale overlay"
    );
    mock.shutdown().await;
}

#[tokio::test]
async fn evaluate_no_http_client_surfaces_setup_alert() {
    // When the CLI has no engine URL configured, every HTTP-backed
    // command must refuse cleanly with a setup hint rather than
    // crash. /evaluate joins the rest of the cohort.
    let ctx = DispatchContext::new(None, EngineState::shared());
    let out = dispatch(&ctx, "/evaluate BTC").await.unwrap().unwrap();
    assert!(out.show_overlay.is_none());
    let line = &out.lines[0];
    let OutputLine::Alert(s) = line else {
        panic!("expected Alert on missing client, got {line:?}");
    };
    assert!(s.contains("engine client"), "setup hint: {s}");
}

// -------------------------------------------------------------
// /pulse
// -------------------------------------------------------------

#[tokio::test]
async fn pulse_renders_mock_events() {
    let (mock, ctx) = ctx_with_mock().await;
    let out = dispatch(&ctx, "/pulse").await.unwrap().unwrap();
    assert_eq!(out.risk, Some(RiskDirection::Neutral));
    assert!(out.show_overlay.is_none(), "/pulse is inline, not overlay");
    assert!(
        out.lines.len() >= 2,
        "mock returns two events: {:?}",
        out.lines
    );
    let joined = out
        .lines
        .iter()
        .map(|l| match l {
            OutputLine::System(s)
            | OutputLine::Command(s)
            | OutputLine::Warn(s)
            | OutputLine::Alert(s) => s.clone(),
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(joined.contains("BTC"), "{joined}");
    assert!(joined.contains("edge_floor cleared"), "{joined}");
    mock.shutdown().await;
}

#[tokio::test]
async fn pulse_without_http_client_emits_alert() {
    let ctx = DispatchContext::new(None, EngineState::shared());
    let out = dispatch(&ctx, "/pulse").await.unwrap().unwrap();
    let OutputLine::Alert(s) = &out.lines[0] else {
        panic!("expected Alert, got {:?}", out.lines);
    };
    assert!(s.contains("engine client"), "setup hint: {s}");
}

#[tokio::test]
async fn pulse_http_500_surfaces_alert() {
    let (mock, ctx) = ctx_with_mock().await;
    mock.with_overrides(|o| o.force_server_error = true);
    let out = dispatch(&ctx, "/pulse").await.unwrap().unwrap();
    assert!(matches!(out.lines.first(), Some(OutputLine::Alert(s)) if s.contains("pulse")));
    mock.shutdown().await;
}

// -------------------------------------------------------------
// /approaching
// -------------------------------------------------------------

#[tokio::test]
async fn approaching_sorts_by_distance_to_gate() {
    let (mock, ctx) = ctx_with_mock().await;
    let out = dispatch(&ctx, "/approaching").await.unwrap().unwrap();
    assert_eq!(out.risk, Some(RiskDirection::Neutral));
    // Mock returns AVAX (0.04) and LINK (0.07); AVAX must sort first.
    let first = match &out.lines[0] {
        OutputLine::Command(s) => s,
        other => panic!("expected Command, got {other:?}"),
    };
    assert!(
        first.contains("AVAX"),
        "closest-to-gate row must be first: {first}"
    );
    let second = match &out.lines[1] {
        OutputLine::Command(s) => s,
        other => panic!("expected Command, got {other:?}"),
    };
    assert!(second.contains("LINK"), "LINK second: {second}");
    mock.shutdown().await;
}

#[tokio::test]
async fn approaching_without_http_client_emits_alert() {
    let ctx = DispatchContext::new(None, EngineState::shared());
    let out = dispatch(&ctx, "/approaching").await.unwrap().unwrap();
    assert!(matches!(out.lines.first(), Some(OutputLine::Alert(_))));
}

#[tokio::test]
async fn approaching_http_500_surfaces_alert() {
    let (mock, ctx) = ctx_with_mock().await;
    mock.with_overrides(|o| o.force_server_error = true);
    let out = dispatch(&ctx, "/approaching").await.unwrap().unwrap();
    assert!(matches!(out.lines.first(), Some(OutputLine::Alert(s)) if s.contains("approaching")));
    mock.shutdown().await;
}

#[tokio::test]
async fn approaching_404_explains_endpoint_is_missing() {
    // Older engine builds don't expose `/approaching` and return
    // 404. The raw `HttpError::NotFound` formats as "not found:
    // /approaching", which looks like a CLI routing bug. Replace
    // it with a concrete explanation so the operator knows it's
    // a server-side capability gap, not a client bug.
    let (mock, ctx) = ctx_with_mock().await;
    mock.with_overrides(|o| o.force_approaching_not_found = true);
    let out = dispatch(&ctx, "/approaching").await.unwrap().unwrap();
    let OutputLine::Alert(s) = &out.lines[0] else {
        panic!("expected Alert on 404, got {:?}", out.lines);
    };
    assert!(s.contains("approaching"), "alert mentions the command: {s}");
    assert!(
        s.to_lowercase().contains("engine") && (s.contains("not expose") || s.contains("missing")),
        "alert explains engine-side cause: {s}"
    );
    // The raw "not found: /approaching" error-display must NOT
    // leak through — that's the UX we're explicitly replacing.
    assert!(
        !s.contains("not found: /approaching"),
        "raw HttpError leak: {s}"
    );
    mock.shutdown().await;
}

// -------------------------------------------------------------
// /rejections
// -------------------------------------------------------------

#[tokio::test]
async fn rejections_renders_mock_entry() {
    let (mock, ctx) = ctx_with_mock().await;
    let out = dispatch(&ctx, "/rejections").await.unwrap().unwrap();
    assert_eq!(out.risk, Some(RiskDirection::Neutral));
    let OutputLine::Command(s) = &out.lines[0] else {
        panic!("expected Command, got {:?}", out.lines);
    };
    assert!(s.contains("SOL"), "coin: {s}");
    assert!(s.contains("stage2"), "stage: {s}");
    assert!(s.contains("volume"), "reason: {s}");
    mock.shutdown().await;
}

#[tokio::test]
async fn rejections_with_coin_filter_passes_through() {
    // Parser accepts `/rejections BTC`; dispatch hands the coin
    // to the HTTP layer unchanged. The mock ignores the filter,
    // but the round-trip must still produce a Command line.
    let (mock, ctx) = ctx_with_mock().await;
    let out = dispatch(&ctx, "/rejections SOL").await.unwrap().unwrap();
    assert!(matches!(out.lines.first(), Some(OutputLine::Command(_))));
    mock.shutdown().await;
}

#[tokio::test]
async fn rejections_without_http_client_emits_alert() {
    let ctx = DispatchContext::new(None, EngineState::shared());
    let out = dispatch(&ctx, "/rejections").await.unwrap().unwrap();
    assert!(matches!(out.lines.first(), Some(OutputLine::Alert(_))));
}

#[tokio::test]
async fn rejections_http_500_surfaces_alert() {
    let (mock, ctx) = ctx_with_mock().await;
    mock.with_overrides(|o| o.force_server_error = true);
    let out = dispatch(&ctx, "/rejections").await.unwrap().unwrap();
    assert!(matches!(out.lines.first(), Some(OutputLine::Alert(s)) if s.contains("rejections")));
    mock.shutdown().await;
}

#[tokio::test]
async fn kill_is_reducer_even_as_stub() {
    let (mock, ctx) = ctx_with_mock().await;
    let out = dispatch(&ctx, "/kill").await.unwrap().unwrap();
    assert_eq!(out.risk, Some(RiskDirection::Reduces));
    // Stub emits an alert so the operator knows the wiring is not
    // yet live. This is honest — not a silent success.
    matches!(out.lines[0], OutputLine::Alert(_));
    mock.shutdown().await;
}

// -------------------------------------------------------------
// /sessions /resume /fork /save
//
// These commands do not touch the engine; they consult the
// session store via the `SessionSource` trait. The tests below
// use a tiny in-memory source so we can exercise every branch
// (happy path + missing-needle + no-current-session) without
// SQLite. That matches how dispatch uses the trait in
// production — a thin `Arc<dyn SessionSource>`.
// -------------------------------------------------------------

#[derive(Default, Debug)]
struct TestSessions {
    inner: Mutex<TestInner>,
}

#[derive(Default, Debug)]
struct TestInner {
    rows: Vec<SessionSummary>,
    events: std::collections::HashMap<String, Vec<ReplayEvent>>,
    labels: std::collections::HashMap<String, String>,
    current: Option<String>,
}

impl TestSessions {
    fn new(current: &str) -> Arc<Self> {
        let s = Self {
            inner: Mutex::new(TestInner {
                current: Some(current.to_string()),
                ..TestInner::default()
            }),
        };
        Arc::new(s)
    }

    fn add(&self, row: SessionSummary, events: Vec<ReplayEvent>) {
        let mut g = self.inner.lock().unwrap();
        g.events.insert(row.ulid.clone(), events);
        g.rows.push(row);
        g.rows.sort_by(|a, b| b.started_at_ms.cmp(&a.started_at_ms));
    }
}

impl SessionSource for TestSessions {
    fn current_ulid(&self) -> Option<String> {
        self.inner.lock().unwrap().current.clone()
    }
    fn list(&self, limit: u32) -> Result<Vec<SessionSummary>, SessionError> {
        let g = self.inner.lock().unwrap();
        Ok(g.rows
            .iter()
            .take(usize::try_from(limit).unwrap_or(usize::MAX))
            .cloned()
            .collect())
    }
    fn find(&self, needle: &str) -> Result<SessionSummary, SessionError> {
        let g = self.inner.lock().unwrap();
        let ulid = g
            .labels
            .get(needle)
            .cloned()
            .or_else(|| {
                g.rows
                    .iter()
                    .find(|s| s.ulid == needle)
                    .map(|s| s.ulid.clone())
            })
            .ok_or(SessionError::NotFound)?;
        g.rows
            .iter()
            .find(|s| s.ulid == ulid)
            .cloned()
            .ok_or(SessionError::NotFound)
    }
    fn list_events(&self, ulid: &str, limit: u32) -> Result<Vec<ReplayEvent>, SessionError> {
        Ok(self
            .inner
            .lock()
            .unwrap()
            .events
            .get(ulid)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .take(usize::try_from(limit).unwrap_or(usize::MAX))
            .collect())
    }
    fn save_label(&self, ulid: &str, label: &str) -> Result<(), SessionError> {
        self.inner
            .lock()
            .unwrap()
            .labels
            .insert(label.to_string(), ulid.to_string());
        Ok(())
    }
    fn fork_from_current(&self) -> Result<Option<String>, SessionError> {
        let mut g = self.inner.lock().unwrap();
        let Some(parent) = g.current.clone() else {
            return Ok(None);
        };
        let child = format!("{parent}X");
        g.current = Some(child.clone());
        let next_ts = g.rows.first().map_or(0, |r| r.started_at_ms + 1);
        g.rows.insert(
            0,
            SessionSummary {
                ulid: child.clone(),
                started_at_ms: next_ts,
                ended_at_ms: None,
                engine_base_url: None,
                cli_version: "test".into(),
                parent_ulid: Some(parent),
                n_events: 0,
            },
        );
        Ok(Some(child))
    }
}

fn ctx_with_sessions(sessions: Arc<TestSessions>) -> DispatchContext {
    DispatchContext::new(None, EngineState::shared()).with_sessions(sessions)
}

#[tokio::test]
async fn sessions_without_store_emits_alert() {
    let ctx = DispatchContext::new(None, EngineState::shared());
    let out = dispatch(&ctx, "/sessions").await.unwrap().unwrap();
    // Persistence disabled → single alert line, no state touched.
    assert!(matches!(out.lines.first(), Some(OutputLine::Alert(s)) if s.contains("persistence")));
    assert!(out.replay_lines.is_empty());
}

#[tokio::test]
async fn sessions_empty_store_emits_honest_empty_state() {
    let src = TestSessions::new("01HX");
    let ctx = ctx_with_sessions(src);
    let out = dispatch(&ctx, "/sessions").await.unwrap().unwrap();
    assert!(matches!(
        out.lines.first(),
        Some(OutputLine::System(s)) if s.contains("no prior sessions")
    ));
}

#[tokio::test]
async fn sessions_lists_newest_first_with_current_marker() {
    let src = TestSessions::new("01HB");
    src.add(
        SessionSummary {
            ulid: "01HA".into(),
            started_at_ms: 1_700_000_000_000,
            ended_at_ms: Some(1_700_000_600_000),
            engine_base_url: None,
            cli_version: "0.3.0".into(),
            parent_ulid: None,
            n_events: 7,
        },
        vec![],
    );
    src.add(
        SessionSummary {
            ulid: "01HB".into(),
            started_at_ms: 1_700_001_000_000,
            ended_at_ms: None,
            engine_base_url: None,
            cli_version: "0.3.0".into(),
            parent_ulid: None,
            n_events: 3,
        },
        vec![],
    );
    let ctx = ctx_with_sessions(src);
    let out = dispatch(&ctx, "/sessions").await.unwrap().unwrap();
    // Header + two rows.
    assert_eq!(out.lines.len(), 3);
    let row_a = match &out.lines[1] {
        OutputLine::System(s) => s.clone(),
        other => panic!("expected System row, got {other:?}"),
    };
    assert!(row_a.contains("01HB"), "first row must be newest: {row_a}");
    assert!(
        row_a.trim_start().starts_with('*'),
        "current session must be flagged with *: {row_a}",
    );
}

#[tokio::test]
async fn resume_missing_needle_prints_usage_hint() {
    let src = TestSessions::new("01HX");
    let ctx = ctx_with_sessions(src);
    let out = dispatch(&ctx, "/resume").await.unwrap().unwrap();
    assert!(matches!(
        out.lines.first(),
        Some(OutputLine::System(s)) if s.contains("ulid|label")
    ));
    assert!(
        out.replay_lines.is_empty(),
        "no events must replay on usage hint"
    );
}

#[tokio::test]
async fn resume_unknown_needle_surfaces_alert() {
    let src = TestSessions::new("01HX");
    let ctx = ctx_with_sessions(src);
    let out = dispatch(&ctx, "/resume nope").await.unwrap().unwrap();
    assert!(matches!(out.lines.first(), Some(OutputLine::Alert(s)) if s.contains("no session")));
}

#[tokio::test]
async fn resume_emits_banner_plus_replay_lines() {
    let src = TestSessions::new("01HNEW");
    src.add(
        SessionSummary {
            ulid: "01HOLD".into(),
            started_at_ms: 1_700_000_000_000,
            ended_at_ms: Some(1_700_000_900_000),
            engine_base_url: None,
            cli_version: "0.3.0".into(),
            parent_ulid: None,
            n_events: 2,
        },
        vec![
            ReplayEvent {
                kind: ReplayKind::Prompt,
                at_ms: 1_700_000_000_500,
                text: "> /status".into(),
            },
            ReplayEvent {
                kind: ReplayKind::Command,
                at_ms: 1_700_000_001_000,
                text: "regime=trend".into(),
            },
        ],
    );
    let ctx = ctx_with_sessions(src);
    let out = dispatch(&ctx, "/resume 01HOLD").await.unwrap().unwrap();
    // Exactly one banner line, rendered as Command so the UI
    // styles it as a structured event rather than a system note.
    assert_eq!(out.lines.len(), 1);
    assert!(matches!(&out.lines[0], OutputLine::Command(s) if s.contains("resuming 01HOLD")));
    // Replay lines carry the original kinds + timestamps.
    assert_eq!(out.replay_lines.len(), 2);
    assert_eq!(out.replay_lines[0].kind, ReplayKind::Prompt);
    assert_eq!(out.replay_lines[0].at_ms, 1_700_000_000_500);
    assert_eq!(out.replay_lines[1].text, "regime=trend");
}

#[tokio::test]
async fn fork_without_current_surfaces_alert() {
    // An impl with no current session — simulates `--no-persist`
    // at runtime or an adapter that lost its active row.
    #[derive(Default)]
    struct Empty;
    impl SessionSource for Empty {
        fn current_ulid(&self) -> Option<String> {
            None
        }
        fn list(&self, _: u32) -> Result<Vec<SessionSummary>, SessionError> {
            Ok(vec![])
        }
        fn find(&self, _: &str) -> Result<SessionSummary, SessionError> {
            Err(SessionError::NotFound)
        }
        fn list_events(&self, _: &str, _: u32) -> Result<Vec<ReplayEvent>, SessionError> {
            Ok(vec![])
        }
        fn save_label(&self, _: &str, _: &str) -> Result<(), SessionError> {
            Ok(())
        }
        fn fork_from_current(&self) -> Result<Option<String>, SessionError> {
            Ok(None)
        }
    }
    let ctx = DispatchContext::new(None, EngineState::shared())
        .with_sessions(Arc::new(Empty) as Arc<dyn SessionSource>);
    let out = dispatch(&ctx, "/fork").await.unwrap().unwrap();
    assert!(matches!(out.lines.first(), Some(OutputLine::Alert(s)) if s.contains("no current")));
}

#[tokio::test]
async fn fork_echoes_new_ulid_and_swaps_current() {
    let src = TestSessions::new("01HPARENT");
    let ctx = ctx_with_sessions(Arc::clone(&src));
    let out = dispatch(&ctx, "/fork").await.unwrap().unwrap();
    let line = match &out.lines[0] {
        OutputLine::Command(s) => s.clone(),
        other => panic!("expected Command, got {other:?}"),
    };
    assert!(line.contains("/fork"), "line: {line}");
    assert!(
        line.contains("01HPARENTX"),
        "new ulid should appear: {line}"
    );
    // Current must have swapped so subsequent /save targets the
    // fork, not the parent.
    assert_eq!(src.current_ulid().as_deref(), Some("01HPARENTX"));
}

#[tokio::test]
async fn save_without_label_prints_usage_hint() {
    let src = TestSessions::new("01HX");
    let ctx = ctx_with_sessions(src);
    let out = dispatch(&ctx, "/save").await.unwrap().unwrap();
    assert!(matches!(out.lines.first(), Some(OutputLine::System(s)) if s.contains("<label>")));
}

#[tokio::test]
async fn replay_without_store_emits_alert() {
    let ctx = DispatchContext::new(None, EngineState::shared());
    let out = dispatch(&ctx, "/replay 01HX").await.unwrap().unwrap();
    assert!(matches!(
        out.lines.first(),
        Some(OutputLine::Alert(s)) if s.contains("/replay") && s.contains("persistence")
    ));
    assert!(out.replay_lines.is_empty());
}

#[tokio::test]
async fn replay_missing_needle_prints_usage_hint_with_replay_verb() {
    let src = TestSessions::new("01HX");
    let ctx = ctx_with_sessions(src);
    let out = dispatch(&ctx, "/replay").await.unwrap().unwrap();
    // The usage line must say "/replay" not "/resume" — the
    // operator-visible verb has to match the command they typed.
    assert!(matches!(
        out.lines.first(),
        Some(OutputLine::System(s))
            if s.contains("/replay") && s.contains("ulid|label")
    ));
}

#[tokio::test]
async fn replay_emits_replaying_banner_and_does_not_switch_active() {
    let src = TestSessions::new("01HCURRENT");
    src.add(
        SessionSummary {
            ulid: "01HOLD".into(),
            started_at_ms: 1_700_000_000_000,
            ended_at_ms: Some(1_700_000_900_000),
            engine_base_url: None,
            cli_version: "0.3.0".into(),
            parent_ulid: None,
            n_events: 1,
        },
        vec![ReplayEvent {
            kind: ReplayKind::System,
            at_ms: 1_700_000_000_500,
            text: "boot".into(),
        }],
    );
    let ctx = ctx_with_sessions(Arc::clone(&src));
    let out = dispatch(&ctx, "/replay 01HOLD").await.unwrap().unwrap();
    // Banner must use the "replaying" verb, not "resuming".
    assert!(matches!(
        &out.lines[0],
        OutputLine::Command(s) if s.contains("replaying 01HOLD")
    ));
    assert_eq!(out.replay_lines.len(), 1);
    // Critically: the current session must be unchanged so
    // `/replay` stays non-destructive.
    assert_eq!(src.current_ulid().as_deref(), Some("01HCURRENT"));
}

#[tokio::test]
async fn share_without_store_emits_alert() {
    let ctx = DispatchContext::new(None, EngineState::shared());
    let out = dispatch(&ctx, "/share").await.unwrap().unwrap();
    assert!(matches!(out.lines.first(), Some(OutputLine::Alert(s)) if s.contains("/share")));
}

#[tokio::test]
async fn share_unknown_needle_surfaces_alert() {
    let src = TestSessions::new("01HX");
    src.add(
        SessionSummary {
            ulid: "01HX".into(),
            started_at_ms: 1_700_000_000_000,
            ended_at_ms: None,
            engine_base_url: None,
            cli_version: "0.3.0".into(),
            parent_ulid: None,
            n_events: 0,
        },
        vec![],
    );
    let ctx = ctx_with_sessions(src);
    let out = dispatch(&ctx, "/share nope").await.unwrap().unwrap();
    assert!(matches!(out.lines.first(), Some(OutputLine::Alert(s)) if s.contains("no session")));
}

#[tokio::test]
async fn share_current_session_emits_header_and_json_block() {
    let src = TestSessions::new("01HCUR");
    src.add(
        SessionSummary {
            ulid: "01HCUR".into(),
            started_at_ms: 1_700_000_000_000,
            ended_at_ms: None,
            engine_base_url: Some("http://e:8080".into()),
            cli_version: "0.3.0".into(),
            parent_ulid: Some("01HPREV".into()),
            n_events: 2,
        },
        vec![
            ReplayEvent {
                kind: ReplayKind::Prompt,
                at_ms: 1_700_000_000_500,
                text: "> /status".into(),
            },
            ReplayEvent {
                kind: ReplayKind::Command,
                at_ms: 1_700_000_001_000,
                text: "regime=trend".into(),
            },
        ],
    );
    let ctx = ctx_with_sessions(src);
    // No argument — defaults to the current session.
    let out = dispatch(&ctx, "/share").await.unwrap().unwrap();
    assert_eq!(out.lines.len(), 2);
    // Header is a Command line so the palette styles it.
    let OutputLine::Command(hdr) = &out.lines[0] else {
        panic!("expected Command header, got {:?}", out.lines[0]);
    };
    assert!(hdr.contains("01HCUR"), "header ulid: {hdr}");
    assert!(hdr.contains("2 event(s)"), "header count: {hdr}");
    // Body is a System line carrying the JSON. Parse it to
    // prove the shape is honest; a string-substring check would
    // let format drift slip through.
    let OutputLine::System(body) = &out.lines[1] else {
        panic!("expected System body, got {:?}", out.lines[1]);
    };
    let v: serde_json::Value = serde_json::from_str(body).expect("share body must be valid JSON");
    assert_eq!(v["ulid"], "01HCUR");
    assert_eq!(v["engine_base_url"], "http://e:8080");
    assert_eq!(v["parent_ulid"], "01HPREV");
    let events = v["events"].as_array().expect("events array");
    assert_eq!(events.len(), 2);
    assert_eq!(events[0]["kind"], "prompt");
    assert_eq!(events[0]["text"], "> /status");
    assert_eq!(events[1]["kind"], "command");
}

#[tokio::test]
async fn share_explicit_ulid_overrides_current_session() {
    let src = TestSessions::new("01HCUR");
    src.add(
        SessionSummary {
            ulid: "01HCUR".into(),
            started_at_ms: 1,
            ended_at_ms: None,
            engine_base_url: None,
            cli_version: "0.3.0".into(),
            parent_ulid: None,
            n_events: 0,
        },
        vec![],
    );
    src.add(
        SessionSummary {
            ulid: "01HOLD".into(),
            started_at_ms: 2,
            ended_at_ms: Some(3),
            engine_base_url: None,
            cli_version: "0.3.0".into(),
            parent_ulid: None,
            n_events: 1,
        },
        vec![ReplayEvent {
            kind: ReplayKind::System,
            at_ms: 2,
            text: "historical".into(),
        }],
    );
    let ctx = ctx_with_sessions(src);
    let out = dispatch(&ctx, "/share 01HOLD").await.unwrap().unwrap();
    let OutputLine::System(body) = &out.lines[1] else {
        panic!("expected System body, got {:?}", out.lines[1]);
    };
    let v: serde_json::Value = serde_json::from_str(body).unwrap();
    assert_eq!(v["ulid"], "01HOLD", "explicit arg must override current");
    assert_eq!(v["events"][0]["text"], "historical");
}

#[tokio::test]
async fn save_echoes_label_to_current_ulid_and_resolves_on_find() {
    let src = TestSessions::new("01HX");
    src.add(
        SessionSummary {
            ulid: "01HX".into(),
            started_at_ms: 1,
            ended_at_ms: None,
            engine_base_url: None,
            cli_version: "0.3.0".into(),
            parent_ulid: None,
            n_events: 0,
        },
        vec![],
    );
    let ctx = ctx_with_sessions(Arc::clone(&src));
    let out = dispatch(&ctx, "/save pre-cpi").await.unwrap().unwrap();
    let line = match &out.lines[0] {
        OutputLine::Command(s) => s.clone(),
        other => panic!("expected Command, got {other:?}"),
    };
    assert!(line.contains("pre-cpi"));
    assert!(line.contains("01HX"));
    // Label must now resolve through `find` for the /resume path.
    let hit = src.find("pre-cpi").unwrap();
    assert_eq!(hit.ulid, "01HX");
}

// ── /config show + /config doctor ───────────────────────────────

fn ctx_with_config(src: Arc<MockConfig>) -> DispatchContext {
    let ctx = DispatchContext::new(None, zero_engine_client::EngineState::shared());
    ctx.with_config(src)
}

#[tokio::test]
async fn config_bare_emits_usage_warn() {
    // No source attached on purpose: usage hints must land
    // even before a source is wired, because operators often
    // learn the subcommand by mistyping `/config` first.
    let ctx = DispatchContext::new(None, zero_engine_client::EngineState::shared());
    let out = dispatch(&ctx, "/config").await.unwrap().unwrap();
    assert_eq!(out.lines.len(), 1);
    let OutputLine::Warn(s) = &out.lines[0] else {
        panic!("expected Warn, got {:?}", out.lines);
    };
    assert!(s.starts_with("/config"));
    assert!(s.contains("show"));
    assert!(s.contains("doctor"));
}

#[tokio::test]
async fn config_unknown_action_names_what_was_rejected() {
    let ctx = DispatchContext::new(None, zero_engine_client::EngineState::shared());
    let out = dispatch(&ctx, "/config secrets").await.unwrap().unwrap();
    let OutputLine::Warn(s) = &out.lines[0] else {
        panic!("expected Warn, got {:?}", out.lines);
    };
    assert!(
        s.contains("'secrets'"),
        "usage line names the bad token: {s}"
    );
}

#[tokio::test]
async fn config_show_alerts_when_no_source_attached() {
    // /config show with no adapter wired is an operational
    // misconfiguration — surface as Alert so the operator
    // sees it immediately (they are asking the tool to tell
    // them the truth about their setup; silent success would
    // be exactly the wrong failure mode).
    let ctx = DispatchContext::new(None, zero_engine_client::EngineState::shared());
    let out = dispatch(&ctx, "/config show").await.unwrap().unwrap();
    assert_eq!(out.lines.len(), 1);
    assert!(matches!(out.lines[0], OutputLine::Alert(_)));
}

#[tokio::test]
async fn config_show_renders_every_row() {
    let src = Arc::new(
        MockConfig::new()
            .with_row("handle", "forge")
            .with_row("api_url", "https://api.getzero.dev")
            .with_row("theme", "phosphor"),
    );
    let ctx = ctx_with_config(src);
    let out = dispatch(&ctx, "/config show").await.unwrap().unwrap();
    // Header + N rows = N+1 lines. Every row is System so the
    // header (Command) is the only non-system line.
    assert_eq!(out.lines.len(), 4, "header + 3 rows: {:?}", out.lines);
    let OutputLine::Command(header) = &out.lines[0] else {
        panic!("expected Command header, got {:?}", out.lines[0]);
    };
    assert!(
        header.contains("3 field(s)"),
        "header reports count: {header}"
    );
    // Every value should be present verbatim.
    let body: Vec<&str> = out.lines[1..]
        .iter()
        .map(|l| match l {
            OutputLine::System(s) => s.as_str(),
            other => panic!("expected System row, got {other:?}"),
        })
        .collect();
    assert!(body.iter().any(|s| s.contains("forge")));
    assert!(body.iter().any(|s| s.contains("api.getzero.dev")));
    assert!(body.iter().any(|s| s.contains("phosphor")));
}

#[tokio::test]
async fn config_show_empty_rows_is_honest() {
    // Fresh install → the adapter returns no rows. The
    // command must not claim "shown 0 fields" because that
    // is still misleading; we steer the operator to `zero
    // init`.
    let src = Arc::new(MockConfig::new());
    let ctx = ctx_with_config(src);
    let out = dispatch(&ctx, "/config show").await.unwrap().unwrap();
    assert_eq!(out.lines.len(), 1);
    let OutputLine::System(s) = &out.lines[0] else {
        panic!("expected System, got {:?}", out.lines);
    };
    assert!(s.contains("no config loaded"));
    assert!(s.contains("zero init"));
}

#[tokio::test]
async fn config_doctor_header_promotes_on_errors() {
    let src = Arc::new(
        MockConfig::new()
            .with_finding(ConfigDoctorFinding::ok("config file readable"))
            .with_finding(ConfigDoctorFinding::warn("default theme; no override set"))
            .with_finding(ConfigDoctorFinding::error("engine token missing")),
    );
    let ctx = ctx_with_config(src);
    let out = dispatch(&ctx, "/config doctor").await.unwrap().unwrap();
    // Header (Alert because there is at least one error) + 3 findings.
    assert_eq!(out.lines.len(), 4);
    let OutputLine::Alert(header) = &out.lines[0] else {
        panic!("expected Alert header, got {:?}", out.lines[0]);
    };
    assert!(
        header.contains("errors=1"),
        "header advertises error count: {header}"
    );
    assert!(
        header.contains("warnings=1"),
        "header advertises warning count: {header}"
    );
    // Severity-specific routing: Ok→System, Warn→Warn, Error→Alert.
    assert!(matches!(out.lines[1], OutputLine::System(_)));
    assert!(matches!(out.lines[2], OutputLine::Warn(_)));
    assert!(matches!(out.lines[3], OutputLine::Alert(_)));
}

#[tokio::test]
async fn config_doctor_clean_run_is_command_header() {
    let src = Arc::new(
        MockConfig::new()
            .with_finding(ConfigDoctorFinding::ok("config file readable"))
            .with_finding(ConfigDoctorFinding::ok("engine token resolvable")),
    );
    let ctx = ctx_with_config(src);
    let out = dispatch(&ctx, "/config doctor").await.unwrap().unwrap();
    assert_eq!(out.lines.len(), 3);
    let OutputLine::Command(header) = &out.lines[0] else {
        panic!("expected Command header on clean run, got {:?}", out.lines);
    };
    assert!(header.contains("errors=0"));
    assert!(header.contains("warnings=0"));
}

#[tokio::test]
async fn config_doctor_no_source_alerts() {
    let ctx = DispatchContext::new(None, zero_engine_client::EngineState::shared());
    let out = dispatch(&ctx, "/config doctor").await.unwrap().unwrap();
    assert_eq!(out.lines.len(), 1);
    assert!(matches!(out.lines[0], OutputLine::Alert(_)));
}

// ── /verbose ────────────────────────────────────────────────────

#[tokio::test]
async fn verbose_toggle_flips_context_state() {
    // Starting from `verbose=false`, a bare `/verbose` must
    // emit an absolute `true` so the TUI does not have to
    // implement toggle semantics. Starting from `true` flips
    // to `false`. Both paths emit exactly one System line —
    // the operator's eyes get a confirmation, never silence.
    let off =
        DispatchContext::new(None, zero_engine_client::EngineState::shared()).with_verbose(false);
    let out = dispatch(&off, "/verbose").await.unwrap().unwrap();
    assert_eq!(out.verbose_toggle, Some(true));
    assert_eq!(out.lines.len(), 1);
    let OutputLine::System(s) = &out.lines[0] else {
        panic!("expected System, got {:?}", out.lines);
    };
    assert_eq!(s, "verbose on");

    let on =
        DispatchContext::new(None, zero_engine_client::EngineState::shared()).with_verbose(true);
    let out = dispatch(&on, "/verbose").await.unwrap().unwrap();
    assert_eq!(out.verbose_toggle, Some(false));
    let OutputLine::System(s) = &out.lines[0] else {
        unreachable!();
    };
    assert_eq!(s, "verbose off");
}

#[tokio::test]
async fn verbose_on_and_off_are_idempotent_but_still_confirm() {
    // `/verbose on` when already on should still emit the
    // confirmation line + toggle intent — silence would make
    // the command look broken.
    let on =
        DispatchContext::new(None, zero_engine_client::EngineState::shared()).with_verbose(true);
    let out = dispatch(&on, "/verbose on").await.unwrap().unwrap();
    assert_eq!(out.verbose_toggle, Some(true));

    let off =
        DispatchContext::new(None, zero_engine_client::EngineState::shared()).with_verbose(false);
    let out = dispatch(&off, "/verbose off").await.unwrap().unwrap();
    assert_eq!(out.verbose_toggle, Some(false));
}

#[tokio::test]
async fn verbose_unknown_argument_is_warn_with_usage() {
    let ctx = DispatchContext::new(None, zero_engine_client::EngineState::shared());
    let out = dispatch(&ctx, "/verbose maybe").await.unwrap().unwrap();
    assert_eq!(out.verbose_toggle, None, "no intent on bad input");
    let OutputLine::Warn(s) = &out.lines[0] else {
        panic!("expected Warn, got {:?}", out.lines);
    };
    assert!(s.contains("'maybe'"));
    assert!(s.contains("on|off|toggle"));
}

// ── Addendum A cohort ───────────────────────────────────────────
//
// Six commands, six shapes. The assertions below pin the *honest*
// contract of each stub: an explicit line the operator can read
// saying exactly what landed and what is still pending. If a
// future wiring-up changes the line text, these tests will fail
// loudly — that is the point. Silent drift from "stub" to
// "pretends it worked" is the failure mode we are fencing out.

use zero_commands::{FrictionDecision, StaticLabel};
use zero_operator_state::Label;

fn ctx_steady() -> DispatchContext {
    DispatchContext::new(None, zero_engine_client::EngineState::shared())
        .with_state(Arc::new(StaticLabel(Label::Steady)))
}

fn ctx_tilt() -> DispatchContext {
    DispatchContext::new(None, zero_engine_client::EngineState::shared())
        .with_state(Arc::new(StaticLabel(Label::Tilt)))
}

#[tokio::test]
async fn state_override_under_steady_proceeds_and_names_label() {
    // At STEADY the friction ladder proceeds immediately. The
    // handler must name the declared label in the output so the
    // override is legible in the audit trail, and must flag the
    // missing engine POST so operators do not infer their claim
    // already reached the classifier.
    let ctx = ctx_steady();
    let out = dispatch(&ctx, "/state-override STEADY")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(out.risk, Some(RiskDirection::Increases));
    assert!(matches!(out.friction, Some(FrictionDecision::Proceed)));
    let OutputLine::Command(s) = &out.lines[0] else {
        panic!("expected Command, got {:?}", out.lines);
    };
    assert!(s.contains("STEADY"));
    assert!(s.contains("pending"));
}

#[tokio::test]
async fn state_override_under_tilt_holds_at_friction_gate() {
    // Asymmetry invariant: the ladder must gate Increases at
    // TILT. `/state-override STEADY` while TILT is exactly the
    // scenario the friction exists to catch (operator self-
    // reporting healthier than observed). Dispatcher must not
    // run the handler in this path; the command rides back as
    // `pending_command` for post-confirm re-dispatch.
    let ctx = ctx_tilt();
    let out = dispatch(&ctx, "/state-override STEADY")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(out.risk, Some(RiskDirection::Increases));
    assert!(matches!(
        out.friction,
        Some(FrictionDecision::TypedConfirm { .. })
    ));
    assert!(out.pending_command.is_some());
}

#[tokio::test]
async fn state_override_without_label_emits_usage_hint() {
    let ctx = ctx_steady();
    let out = dispatch(&ctx, "/state-override").await.unwrap().unwrap();
    let OutputLine::Warn(s) = &out.lines[0] else {
        panic!("expected Warn usage, got {:?}", out.lines);
    };
    assert!(s.contains("FRESH"));
    assert!(s.contains("STEADY"));
    assert!(s.contains("TILT"));
}

#[tokio::test]
async fn state_override_with_unknown_label_emits_usage_hint() {
    let ctx = ctx_steady();
    let out = dispatch(&ctx, "/state-override slurpy")
        .await
        .unwrap()
        .unwrap();
    let OutputLine::Warn(s) = &out.lines[0] else {
        panic!("expected Warn usage, got {:?}", out.lines);
    };
    assert!(s.contains("STEADY"));
}

#[tokio::test]
async fn continue_acknowledges_without_queue() {
    let ctx = DispatchContext::new(None, zero_engine_client::EngineState::shared());
    let out = dispatch(&ctx, "/continue").await.unwrap().unwrap();
    assert_eq!(out.risk, Some(RiskDirection::Neutral));
    let OutputLine::System(s) = &out.lines[0] else {
        panic!("expected System, got {:?}", out.lines);
    };
    assert!(s.contains("acknowledged"));
    assert!(s.contains("pending"));
}

#[tokio::test]
async fn close_requires_a_coin() {
    let ctx = DispatchContext::new(None, zero_engine_client::EngineState::shared());
    let out = dispatch(&ctx, "/close").await.unwrap().unwrap();
    assert_eq!(out.risk, Some(RiskDirection::Reduces));
    let OutputLine::Warn(s) = &out.lines[0] else {
        panic!("expected Warn, got {:?}", out.lines);
    };
    assert!(s.contains("<coin>"));
    assert!(s.contains("/flatten-all"));
}

#[tokio::test]
async fn close_with_coin_acknowledges_and_tags_pending() {
    let ctx = DispatchContext::new(None, zero_engine_client::EngineState::shared());
    let out = dispatch(&ctx, "/close BTC").await.unwrap().unwrap();
    assert_eq!(out.risk, Some(RiskDirection::Reduces));
    let OutputLine::System(s) = &out.lines[0] else {
        panic!("expected System, got {:?}", out.lines);
    };
    assert!(s.contains("BTC"));
    assert!(s.contains("pending"));
    assert!(
        s.contains("no order was placed"),
        "must be honest the order did not go out"
    );
}

#[tokio::test]
async fn close_is_friction_exempt_even_at_tilt() {
    // The whole point of /close being tagged Reduces is that the
    // friction ladder can never gate it. This test is the
    // canary for that invariant at the dispatch boundary —
    // `friction::tests::reduces_never_gated_regardless_of_label`
    // covers the ladder in isolation; this one guards the
    // integration.
    let ctx = ctx_tilt();
    let out = dispatch(&ctx, "/close BTC").await.unwrap().unwrap();
    assert!(matches!(out.friction, Some(FrictionDecision::Proceed)));
    assert!(out.pending_command.is_none());
}

#[tokio::test]
async fn wrap_off_emits_absolute_toggle_and_confirmation() {
    let ctx = DispatchContext::new(None, zero_engine_client::EngineState::shared());
    let out = dispatch(&ctx, "/wrap-off").await.unwrap().unwrap();
    assert_eq!(out.wrap_off_toggle, Some(true));
    let OutputLine::System(s) = &out.lines[0] else {
        panic!("expected System, got {:?}", out.lines);
    };
    assert!(s.contains("this session"));
    assert!(
        s.contains("next session"),
        "honest about the non-sticky nature",
    );
}

#[tokio::test]
async fn coaching_reset_signals_buffer_clear() {
    let ctx = DispatchContext::new(None, zero_engine_client::EngineState::shared());
    let out = dispatch(&ctx, "/coaching reset").await.unwrap().unwrap();
    assert!(out.coaching_reset);
    let OutputLine::System(s) = &out.lines[0] else {
        panic!("expected System, got {:?}", out.lines);
    };
    assert!(s.contains("cleared"));
}

#[tokio::test]
async fn coaching_without_subcommand_is_unknown() {
    let ctx = DispatchContext::new(None, zero_engine_client::EngineState::shared());
    let out = dispatch(&ctx, "/coaching").await.unwrap().unwrap();
    // Unknown path → Warn "unknown command".
    assert!(matches!(out.lines[0], OutputLine::Warn(_)));
}

#[tokio::test]
async fn zero_prefix_teaches_instead_of_unknown_command() {
    // Paper cut we saw in a live session: an operator hit
    // `engine:RECONNECTING retry:N` on the status bar, tried
    // `zero doctor` at the prompt (the same phrase the README
    // told them to run outside the TUI), and got "unknown
    // command: /zero". That made the tool look broken on top
    // of the actual auth problem. These assertions pin the
    // teaching-hint path: each shell-shaped invocation must
    // produce a Warn that (a) names the slash form that DOES
    // exist, or (b) tells them to `/quit` back to the shell
    // when no in-TUI equivalent is reachable.
    let ctx = DispatchContext::new(None, zero_engine_client::EngineState::shared());

    let out = dispatch(&ctx, "zero doctor").await.unwrap().unwrap();
    let OutputLine::Warn(s) = &out.lines[0] else {
        panic!("expected Warn, got {:?}", out.lines);
    };
    assert!(
        s.contains("/doctor"),
        "expected hint to name /doctor, got {s:?}"
    );
    assert!(
        s.contains("already inside zero"),
        "expected teaching voice, got {s:?}"
    );

    let out = dispatch(&ctx, "zero --version").await.unwrap().unwrap();
    let OutputLine::Warn(s) = &out.lines[0] else {
        panic!("expected Warn, got {:?}", out.lines);
    };
    assert!(
        s.contains("/quit"),
        "expected version hint to route operator back to shell, got {s:?}"
    );

    let out = dispatch(&ctx, "zero init --force").await.unwrap().unwrap();
    let OutputLine::Warn(s) = &out.lines[0] else {
        panic!("expected Warn, got {:?}", out.lines);
    };
    // `zero init` has no in-TUI equivalent; the hint must NOT
    // invent a `/init` command (that was the `zero pair` ghost
    // mistake we just fixed in main.rs). It must echo the
    // operator's tail and point at `/quit` + `/help`.
    assert!(
        s.contains("init --force"),
        "expected hint to echo typed tail, got {s:?}"
    );
    assert!(
        !s.contains("/init"),
        "hint must not invent a /init command, got {s:?}"
    );

    let out = dispatch(&ctx, "zero").await.unwrap().unwrap();
    let OutputLine::Warn(s) = &out.lines[0] else {
        panic!("expected Warn, got {:?}", out.lines);
    };
    assert!(
        s.contains("/help"),
        "bare `zero` must point at /help, got {s:?}"
    );
}

#[tokio::test]
async fn disclosure_override_without_phrase_alerts_and_names_it() {
    let ctx = ctx_steady();
    let out = dispatch(&ctx, "/disclosure-override")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(out.risk, Some(RiskDirection::Increases));
    // STEADY proceeds past friction — the alert here is the
    // handler-level guard that still blocks the bypass.
    assert!(matches!(out.friction, Some(FrictionDecision::Proceed)));
    let OutputLine::Alert(s) = &out.lines[0] else {
        panic!("expected Alert, got {:?}", out.lines);
    };
    assert!(s.contains("--i-know-what-i-am-doing"));
}

#[tokio::test]
async fn disclosure_override_with_phrase_under_steady_proceeds() {
    let ctx = ctx_steady();
    let out = dispatch(&ctx, "/disclosure-override --i-know-what-i-am-doing")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(out.risk, Some(RiskDirection::Increases));
    assert!(matches!(out.friction, Some(FrictionDecision::Proceed)));
    let OutputLine::Command(s) = &out.lines[0] else {
        panic!("expected Command, got {:?}", out.lines);
    };
    assert!(s.contains("bypassed"));
    assert!(s.contains("pending"), "disclosure store not wired yet");
}

#[tokio::test]
async fn disclosure_override_at_tilt_is_held_by_friction_not_phrase() {
    // Under TILT, the friction ladder alone is enough to stop
    // the command; the handler never runs so the phrase is
    // never evaluated. The `pending_command` carries the
    // unmodified (confirmed=true) command so the TUI can re-
    // dispatch after typed-confirm.
    let ctx = ctx_tilt();
    let out = dispatch(&ctx, "/disclosure-override --i-know-what-i-am-doing")
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        out.friction,
        Some(FrictionDecision::TypedConfirm { .. })
    ));
    assert!(out.pending_command.is_some());
}

#[tokio::test]
async fn rate_without_any_arguments_emits_usage_hint() {
    // Operators who mistype the command shape should see the
    // full expected form with the range spelled out, not a
    // silent no-op. The hint is anchored on `1..=10` so a
    // future narrowing (e.g. 1..=5) surfaces as a test break.
    let ctx = DispatchContext::new(None, zero_engine_client::EngineState::shared());
    let out = dispatch(&ctx, "/rate").await.unwrap().unwrap();
    assert_eq!(out.risk, Some(RiskDirection::Neutral));
    let OutputLine::Warn(s) = &out.lines[0] else {
        panic!("expected Warn, got {:?}", out.lines);
    };
    assert!(s.contains("<trade_id>"));
    assert!(s.contains("1..=10"));
}

#[tokio::test]
async fn rate_with_id_only_asks_for_a_rating() {
    // The parser binds `trade_id` but leaves `rating` None;
    // the handler must name the specific trade in its hint
    // so the operator sees what the CLI already captured.
    let ctx = DispatchContext::new(None, zero_engine_client::EngineState::shared());
    let out = dispatch(&ctx, "/rate t-001").await.unwrap().unwrap();
    let OutputLine::Warn(s) = &out.lines[0] else {
        panic!("expected Warn, got {:?}", out.lines);
    };
    assert!(s.contains("t-001"));
    assert!(s.contains("1..=10"));
}

#[tokio::test]
async fn rate_out_of_range_at_parser_surfaces_usage_hint() {
    // The parser routes `0` / `11` to the id slot rather than
    // clamping them to `rating`. The dispatcher then sees
    // `rating = None` and emits the same usage hint it would
    // for a missing rating. The honesty property under test:
    // no out-of-range value ever ends up recorded.
    let ctx = DispatchContext::new(None, zero_engine_client::EngineState::shared());
    let out = dispatch(&ctx, "/rate 11").await.unwrap().unwrap();
    let OutputLine::Warn(s) = &out.lines[0] else {
        panic!("expected Warn, got {:?}", out.lines);
    };
    assert!(s.contains("1..=10"));
    // The honest pane line must name the token that *was*
    // captured so the operator can see the parser didn't
    // silently clamp.
    assert!(s.contains("11"));
}

#[tokio::test]
async fn rate_happy_path_without_http_is_honest_about_missing_client() {
    // No-HTTP path — the dispatch context has no engine client
    // (e.g. `--no-engine` mode, tests). The handler still emits
    // a single Command line echoing both the trade id and the
    // rating so the operator can confirm what was captured, but
    // it must flag the absent client so no one misreads the
    // local echo as a successful engine POST.
    let ctx = DispatchContext::new(None, zero_engine_client::EngineState::shared());
    let out = dispatch(&ctx, "/rate t-001 8").await.unwrap().unwrap();
    assert_eq!(out.risk, Some(RiskDirection::Neutral));
    assert!(matches!(out.friction, Some(FrictionDecision::Proceed)));
    let OutputLine::Command(s) = &out.lines[0] else {
        panic!("expected Command, got {:?}", out.lines);
    };
    assert!(s.contains("t-001"));
    assert!(s.contains('8'));
    assert!(s.contains("recorded"));
    assert!(
        s.contains("engine client unavailable"),
        "must be honest about the absent engine client, got: {s:?}"
    );
    assert!(
        !s.contains("posted to engine"),
        "must not claim a POST happened when none did, got: {s:?}"
    );
}

#[tokio::test]
async fn rate_is_friction_exempt_even_at_tilt() {
    // /rate is a self-report about a past trade; it changes
    // no position and must not gate on the friction ladder.
    // A TILT operator reflecting on their own trade is
    // *exactly* the case where a recorded conviction is most
    // useful — gating it would destroy the calibration data.
    let ctx = ctx_tilt();
    let out = dispatch(&ctx, "/rate t-001 3").await.unwrap().unwrap();
    assert!(matches!(out.friction, Some(FrictionDecision::Proceed)));
    assert!(out.pending_command.is_none());
}

#[tokio::test]
async fn rate_with_mock_engine_posts_conviction_event() {
    // Wiring test for the `/rate` → `POST /operator/events`
    // rewire. Asserts three things that together pin the
    // contract:
    //
    //   1. The output line claims the POST happened ("posted
    //      to engine") — the vocabulary the conversation log
    //      indexes on.
    //   2. The mock observed exactly one event body.
    //   3. The body's JSON matches the canonical tagged-union
    //      wire-format: `kind == "conviction"`, `trade_id` and
    //      `rating` echoed verbatim, and a well-formed `ts`
    //      field. The `ts` value is wall-clock at call time so
    //      we do not pin it, but we require the field exist and
    //      parse — a missing or malformed `ts` would make the
    //      engine-side classifier reject the event.
    use chrono::DateTime;
    let (mock, ctx) = ctx_with_mock().await;
    let out = dispatch(&ctx, "/rate t-042 7").await.unwrap().unwrap();
    let OutputLine::Command(s) = &out.lines[0] else {
        panic!("expected Command, got {:?}", out.lines);
    };
    assert!(s.contains("t-042"));
    assert!(s.contains('7'));
    assert!(
        s.contains("posted to engine"),
        "must confirm the POST succeeded, got: {s:?}"
    );
    assert!(
        !s.contains("pending"),
        "must not repeat the pre-rewire 'pending' tag after a successful POST, got: {s:?}"
    );

    let received = mock.received_operator_events();
    assert_eq!(received.len(), 1, "mock saw: {received:?}");
    let body = &received[0];
    assert_eq!(body["kind"], "conviction");
    assert_eq!(body["trade_id"], "t-042");
    assert_eq!(body["rating"], 7);
    let ts = body["ts"].as_str().expect("ts present as string");
    DateTime::parse_from_rfc3339(ts).expect("ts parses as RFC-3339");
    mock.shutdown().await;
}

#[tokio::test]
async fn break_with_mock_engine_posts_break_started_event() {
    // Paired test for `/break`. The parser accepts minutes for
    // ergonomics; the on-the-wire `planned_ms` must be the
    // conversion in milliseconds so the engine-side classifier
    // (native-ms) does not need a per-event unit probe. A bare
    // `/break` with no duration surfaces as `planned_ms: null`
    // (tested by absence of the field in the serialized form —
    // serde emits `null` rather than eliding, which is fine).
    use chrono::DateTime;
    let (mock, ctx) = ctx_with_mock().await;
    // Parser eats minutes as a bare integer (see
    // `command.rs` — `/break 15`, not `/break 15m`). The
    // conversation line *rendered by the handler* then
    // appends the `m` suffix for legibility; tests must
    // anchor on that rendered form, not the input form.
    let out = dispatch(&ctx, "/break 15").await.unwrap().unwrap();
    let OutputLine::System(s) = &out.lines[0] else {
        panic!("expected System, got {:?}", out.lines);
    };
    assert!(s.contains("/break 15m"));
    assert!(
        s.contains("posted to engine"),
        "must confirm the POST succeeded, got: {s:?}"
    );

    let received = mock.received_operator_events();
    assert_eq!(received.len(), 1, "mock saw: {received:?}");
    let body = &received[0];
    assert_eq!(body["kind"], "break_started");
    assert_eq!(body["planned_ms"], 15 * 60_000);
    let ts = body["ts"].as_str().expect("ts present as string");
    DateTime::parse_from_rfc3339(ts).expect("ts parses as RFC-3339");
    mock.shutdown().await;
}

#[tokio::test]
async fn break_without_minutes_posts_null_planned_ms() {
    // A `/break` with no duration — operator hit the break key
    // without specifying how long. `planned_ms` must be `null`
    // (not absent) so the engine-side classifier's
    // `Option<u64>` deserialization has a concrete shape to
    // match. Renders an honest "noted, posted" line.
    let (mock, ctx) = ctx_with_mock().await;
    let out = dispatch(&ctx, "/break").await.unwrap().unwrap();
    let OutputLine::System(s) = &out.lines[0] else {
        panic!("expected System, got {:?}", out.lines);
    };
    assert!(s.contains("posted to engine"), "got: {s:?}");

    let received = mock.received_operator_events();
    assert_eq!(received.len(), 1);
    let body = &received[0];
    assert_eq!(body["kind"], "break_started");
    assert!(
        body.get("planned_ms")
            .is_some_and(serde_json::Value::is_null),
        "planned_ms must be explicit null when no duration given, got: {body:?}"
    );
    mock.shutdown().await;
}
