//! Integration tests for the first-run wizard.
//!
//! Tests cover:
//! - Interactive happy path against a reachable mock engine.
//! - Non-interactive happy path with full flags.
//! - Non-interactive failure when required flags are missing.
//! - Validation rejects bogus URLs / empty handle.
//! - Declined confirmation short-circuits with a clean error.

#![cfg(feature = "testing")]

use zero_onboarding::prompt::MockPrompt;
use zero_onboarding::{Flags, run_interactive, run_non_interactive};
use zero_testkit::mock_engine::MockEngine;

#[tokio::test]
async fn non_interactive_against_live_mock_engine() {
    let mock = MockEngine::spawn().await.expect("mock engine");
    let flags = Flags {
        handle: Some("ada".into()),
        api: Some(mock.base_url()),
        token: Some("t0".into()),
        accept_defaults: true,
    };

    let plan = run_non_interactive(&flags).await.expect("plan");
    assert_eq!(plan.config.identity.handle, "ada");
    assert_eq!(plan.api_url, mock.base_url());
    assert_eq!(plan.token.as_deref(), Some("t0"));
    assert!(plan.engine_reachable, "mock engine must respond to /health");
    // Defensive defaults carried through.
    assert_eq!(plan.config.mode.default, "plan");
    assert!(!plan.config.mode.allow_auto);
    assert!((plan.config.guardrails.max_position_pct - 5.0).abs() < f64::EPSILON);
}

#[tokio::test]
async fn non_interactive_requires_handle_without_yes() {
    let flags = Flags {
        handle: None,
        api: Some("http://localhost:1".into()),
        token: None,
        accept_defaults: false,
    };
    let err = run_non_interactive(&flags).await.expect_err("error");
    assert!(err.to_string().contains("handle"));
}

#[tokio::test]
async fn non_interactive_accepts_operator_default_with_yes() {
    let flags = Flags {
        handle: None,
        api: Some("http://127.0.0.1:1".into()),
        token: None,
        accept_defaults: true,
    };
    let plan = run_non_interactive(&flags).await.expect("plan");
    assert_eq!(plan.config.identity.handle, "operator");
    assert!(!plan.engine_reachable, "nothing is listening on :1");
}

#[tokio::test]
async fn non_interactive_rejects_invalid_url() {
    let flags = Flags {
        handle: Some("x".into()),
        api: Some("not-a-url".into()),
        token: None,
        accept_defaults: true,
    };
    let err = run_non_interactive(&flags).await.expect_err("error");
    assert!(err.to_string().contains("api url"));
}

#[tokio::test]
async fn interactive_happy_path() {
    let mock = MockEngine::spawn().await.expect("mock");
    let mut p = MockPrompt::with_answers([
        "ada".to_string(),
        mock.base_url(),
        "secret-token".to_string(),
    ])
    .with_confirms([true]);

    let plan = run_interactive(&mut p).await.expect("plan");
    assert_eq!(plan.config.identity.handle, "ada");
    assert_eq!(plan.api_url, mock.base_url());
    assert_eq!(plan.token.as_deref(), Some("secret-token"));
    assert!(plan.engine_reachable);

    let transcript = p.transcript.join("\n");
    assert!(transcript.contains("ASK Operator handle"));
    assert!(transcript.contains("ASK Engine API URL"));
    assert!(transcript.contains("SECRET Engine token"));
    assert!(transcript.contains("CONFIRM Write config"));
}

#[tokio::test]
async fn interactive_declined_errors() {
    let mut p = MockPrompt::with_answers([
        "ada".to_string(),
        "http://localhost:1".to_string(),
        String::new(),
    ])
    .with_confirms([false]);

    let err = run_interactive(&mut p).await.expect_err("error");
    assert!(err.to_string().contains("declined"));
}

#[tokio::test]
async fn interactive_empty_handle_rejected() {
    let mut p = MockPrompt::with_answers([
        // Intentionally empty — the wizard prompts with a default
        // of "operator" so an explicit empty here means "nope".
        // Actually MockPrompt returns the pop-front value, not the
        // default; an empty string means the operator typed just
        // whitespace.
        "   ".to_string(),
    ]);

    let err = run_interactive(&mut p).await.expect_err("error");
    assert!(err.to_string().contains("handle"));
}
