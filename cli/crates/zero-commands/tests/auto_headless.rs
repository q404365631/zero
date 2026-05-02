//! M2 §5 — `/auto`, `/headless`, and the `/kill` compound.
//!
//! Thirteen tests organized into four sections:
//!
//! 1. **Parser shapes** — that the `/auto` and `/headless`
//!    subcommands resolve to the expected [`Command`] variants,
//!    including the `Missing` / `Unknown` fall-through arms
//!    that route to usage hints.
//! 2. **Friction gating** — that `/auto on` picks up the
//!    friction ladder at `RiskDirection::Increases` exactly like
//!    `/execute`, while `/auto off` / `/auto status` /
//!    `/headless *` stay at `RiskDirection::Neutral` and pass
//!    the ladder unchanged. The regression this pins is that a
//!    future edit to [`Command::risk`] cannot silently downgrade
//!    `/auto on` without a compile-time + runtime witness.
//! 3. **Adapter calls** — that `/auto` and `/headless`
//!    dispatchers route through the attached
//!    [`AutoSource`] / [`SupervisorSource`] and render the
//!    expected line shapes; that the unattached path emits a
//!    single "unavailable" alert rather than panicking.
//! 4. **`/kill` compound** — that `/kill` tears down the
//!    daemon socket when a supervisor is attached and running,
//!    tags the confirmation line, and falls through to the
//!    legacy single-line form otherwise.

use std::sync::Arc;

use zero_commands::{
    AutoAction, AutoMode, AutoRequest, AutoSource, Command, DispatchContext, FrictionDecision,
    HeadlessAction, MockAutoSource, MockSupervisorSource, OutputLine, RiskDirection, StaticLabel,
    SupervisorAction, SupervisorError, SupervisorReply, SupervisorSource, SupervisorState,
    dispatch, parse_line, resolve,
};
use zero_engine_client::EngineState;
use zero_operator_state::Label;

// ---- small helpers ---------------------------------------------------------

fn ctx_at(label: Label) -> DispatchContext {
    DispatchContext::new(None, EngineState::shared()).with_state(Arc::new(StaticLabel(label)))
}

fn ctx_tilt_with_auto(src: Arc<dyn AutoSource>) -> DispatchContext {
    ctx_at(Label::Tilt).with_auto(src)
}

fn ctx_steady_with_auto(src: Arc<dyn AutoSource>) -> DispatchContext {
    ctx_at(Label::Steady).with_auto(src)
}

fn ctx_with_supervisor(src: Arc<dyn SupervisorSource>) -> DispatchContext {
    DispatchContext::new(None, EngineState::shared()).with_supervisor(src)
}

// ---- 1. parser shapes ------------------------------------------------------

#[test]
fn parser_auto_resolves_on_off_status_and_missing_and_unknown() {
    let r = |s: &str| resolve(&parse_line(s)).unwrap();

    // Canonical verbs — the three paths the catalog advertises.
    assert_eq!(
        r("/auto on"),
        Command::Auto {
            action: AutoAction::On
        }
    );
    assert_eq!(
        r("/auto OFF"), // case-insensitive.
        Command::Auto {
            action: AutoAction::Off
        }
    );
    assert_eq!(
        r("/auto status"),
        Command::Auto {
            action: AutoAction::Status
        }
    );
    // Friendly aliases — scripts and muscle memory reach for
    // booleans, so `true` / `1` / `false` / `0` land on the
    // same variants. `status` / `stat` / `show` all resolve to
    // `Status` for symmetry with `/config`.
    assert_eq!(
        r("/auto true"),
        Command::Auto {
            action: AutoAction::On
        }
    );
    assert_eq!(
        r("/auto 0"),
        Command::Auto {
            action: AutoAction::Off
        }
    );
    assert_eq!(
        r("/auto show"),
        Command::Auto {
            action: AutoAction::Status
        }
    );

    // Bare `/auto` — route to the `Missing` arm so the
    // dispatcher can emit a usage hint. Silent acceptance of a
    // risk-increasing command with no argument would be the
    // exact honesty bug §5 exists to prevent.
    assert_eq!(
        r("/auto"),
        Command::Auto {
            action: AutoAction::Missing
        }
    );

    // Unknown argument — route to `Unknown(token)` so the
    // hint can echo the exact token the operator typed.
    assert_eq!(
        r("/auto wiggle"),
        Command::Auto {
            action: AutoAction::Unknown("wiggle".into())
        }
    );
}

#[test]
fn parser_headless_resolves_start_stop_status_and_missing_and_unknown() {
    let r = |s: &str| resolve(&parse_line(s)).unwrap();

    assert_eq!(
        r("/headless start"),
        Command::Headless {
            action: HeadlessAction::Start
        }
    );
    assert_eq!(
        r("/headless stop"),
        Command::Headless {
            action: HeadlessAction::Stop
        }
    );
    assert_eq!(
        r("/headless STATUS"), // case-insensitive.
        Command::Headless {
            action: HeadlessAction::Status
        }
    );
    // `up` / `down` are adapter-speak aliases — identical to
    // `start` / `stop` on the daemon side. Including them
    // keeps the parser kind to operators who think in
    // supervisor terms rather than CLI verbs.
    assert_eq!(
        r("/headless up"),
        Command::Headless {
            action: HeadlessAction::Start
        }
    );
    assert_eq!(
        r("/headless down"),
        Command::Headless {
            action: HeadlessAction::Stop
        }
    );

    assert_eq!(
        r("/headless"),
        Command::Headless {
            action: HeadlessAction::Missing
        }
    );
    assert_eq!(
        r("/headless restart"),
        Command::Headless {
            action: HeadlessAction::Unknown("restart".into())
        }
    );
}

// ---- 2. friction gating ----------------------------------------------------

#[test]
fn auto_on_classifies_as_risk_increases_off_and_status_as_neutral() {
    // Lock down the `Command::risk` mapping at the type level:
    // a future edit that conflates the three actions silently
    // downgrades the canonical gated action, which is the
    // exact regression a 2 AM operator cannot afford.
    let on = Command::Auto {
        action: AutoAction::On,
    };
    let off = Command::Auto {
        action: AutoAction::Off,
    };
    let status = Command::Auto {
        action: AutoAction::Status,
    };
    let missing = Command::Auto {
        action: AutoAction::Missing,
    };
    let unknown = Command::Auto {
        action: AutoAction::Unknown("foo".into()),
    };

    assert_eq!(on.risk(), RiskDirection::Increases);
    assert_eq!(off.risk(), RiskDirection::Neutral);
    assert_eq!(status.risk(), RiskDirection::Neutral);
    // Missing / Unknown degrade to Neutral so typing the
    // command alone to discover usage does not trip L2
    // friction at TILT. The dispatcher surfaces a usage hint
    // either way.
    assert_eq!(missing.risk(), RiskDirection::Neutral);
    assert_eq!(unknown.risk(), RiskDirection::Neutral);
}

#[tokio::test]
async fn auto_on_at_tilt_triggers_typed_confirm_and_carries_pending() {
    // The canonical proof that `/auto on` joins the friction
    // ladder exactly like `/execute`: at TILT the ladder emits
    // [`FrictionDecision::TypedConfirm`] and carries the
    // resolved `Command::Auto { On }` as `pending_command` so
    // the TUI can re-dispatch via `run_bypass_friction` after
    // the operator honors the pause.
    let src = Arc::new(MockAutoSource::new(AutoMode::Off));
    let ctx = ctx_tilt_with_auto(src.clone());

    let out = dispatch(&ctx, "/auto on").await.unwrap().unwrap();
    assert_eq!(out.risk, Some(RiskDirection::Increases));
    assert!(
        matches!(out.friction, Some(FrictionDecision::TypedConfirm { .. })),
        "expected TypedConfirm at TILT, got {:?}",
        out.friction
    );
    assert_eq!(
        out.pending_command,
        Some(Command::Auto {
            action: AutoAction::On
        }),
    );
    // Critical invariant: the adapter must NOT have been
    // called — friction gates the call, not just the line.
    assert_eq!(
        src.current(),
        AutoMode::Off,
        "adapter must not flip until the operator honors friction",
    );
}

#[tokio::test]
async fn auto_off_and_status_are_never_gated_even_at_tilt() {
    // Neutral commands pass the ladder at every label. The 2 AM
    // invariant: turning the accelerator *off* at TILT is the
    // operator's risk-reducing recovery path, and gating it
    // would be the exact inversion the architecture forbids.
    let src = Arc::new(MockAutoSource::new(AutoMode::On));
    let ctx = ctx_tilt_with_auto(src.clone());

    let off = dispatch(&ctx, "/auto off").await.unwrap().unwrap();
    assert_eq!(off.risk, Some(RiskDirection::Neutral));
    assert!(matches!(off.friction, Some(FrictionDecision::Proceed)));
    assert!(off.pending_command.is_none());
    assert_eq!(src.current(), AutoMode::Off, "adapter ran at TILT");

    let status = dispatch(&ctx, "/auto status").await.unwrap().unwrap();
    assert!(matches!(status.friction, Some(FrictionDecision::Proceed)));
    assert_eq!(status.risk, Some(RiskDirection::Neutral));
}

#[tokio::test]
async fn headless_is_never_gated_regardless_of_label() {
    // All three `/headless` verbs are Neutral — the daemon
    // does not take new positions. A TILT operator spawning
    // the supervisor is the expected recovery posture
    // (daemon-as-watchdog), so gating any variant would be
    // wrong.
    let src = Arc::new(MockSupervisorSource::new(false));
    let ctx = ctx_at(Label::Tilt).with_supervisor(src.clone());

    for line in ["/headless start", "/headless status", "/headless stop"] {
        let out = dispatch(&ctx, line).await.unwrap().unwrap();
        assert_eq!(out.risk, Some(RiskDirection::Neutral), "line = {line}");
        assert!(
            matches!(out.friction, Some(FrictionDecision::Proceed)),
            "line = {line}, got {:?}",
            out.friction
        );
        assert!(out.pending_command.is_none());
    }
}

// ---- 3. adapter calls ------------------------------------------------------

#[tokio::test]
async fn auto_on_at_steady_flips_adapter_and_renders_changed_line() {
    // Happy path — STEADY passes the ladder, the adapter flips
    // the mode, and the dispatcher renders a `(changed)` tag so
    // the operator sees the call had an effect.
    let src = Arc::new(MockAutoSource::new(AutoMode::Off));
    let ctx = ctx_steady_with_auto(src.clone());

    let out = dispatch(&ctx, "/auto on").await.unwrap().unwrap();
    assert!(matches!(out.friction, Some(FrictionDecision::Proceed)));
    assert_eq!(src.current(), AutoMode::On);

    let OutputLine::Command(line) = &out.lines[0] else {
        panic!("expected Command, got {:?}", out.lines);
    };
    assert!(line.contains("mode=on"), "line = {line:?}");
    assert!(line.contains("changed"), "line = {line:?}");
}

#[tokio::test]
async fn auto_on_when_already_on_surfaces_warn_not_alert() {
    // Idempotent flip — nothing broke, but the operator
    // asked for a change that did not happen. Rendered as a
    // warn so the log colour tracks the ambiguity.
    let src = Arc::new(MockAutoSource::new(AutoMode::On));
    let ctx = ctx_steady_with_auto(src.clone());

    let out = dispatch(&ctx, "/auto on").await.unwrap().unwrap();
    let OutputLine::Warn(line) = &out.lines[0] else {
        panic!("expected Warn, got {:?}", out.lines);
    };
    assert!(line.contains("already on"), "line = {line:?}");
}

#[tokio::test]
async fn auto_without_adapter_surfaces_unavailable_alert() {
    // The unattached path — every adapter-less invocation
    // must surface a single honest alert instead of panicking
    // or silently no-op'ing. Parallels `/config` unavailable.
    let ctx = ctx_at(Label::Steady);

    let out = dispatch(&ctx, "/auto on").await.unwrap().unwrap();
    // Risk-increasing is still recorded — the dispatcher ran
    // the ladder even without an adapter (decide_with_risk
    // returns Proceed at STEADY), so `friction=Proceed`.
    assert_eq!(out.risk, Some(RiskDirection::Increases));
    let OutputLine::Alert(line) = &out.lines[0] else {
        panic!("expected Alert, got {:?}", out.lines);
    };
    assert!(line.contains("unavailable"), "line = {line:?}");
}

#[tokio::test]
async fn auto_missing_and_unknown_surface_usage_hints() {
    // Bare `/auto` is Neutral (see `risk_classification` above)
    // so the ladder passes; the dispatcher then emits the
    // usage hint. Unknown token is echoed back verbatim so
    // the hint is actionable.
    let src = Arc::new(MockAutoSource::new(AutoMode::Off));
    let ctx = ctx_steady_with_auto(src.clone());

    let missing = dispatch(&ctx, "/auto").await.unwrap().unwrap();
    let OutputLine::System(m) = &missing.lines[0] else {
        panic!("expected System, got {:?}", missing.lines);
    };
    assert!(m.contains("usage"), "missing line = {m:?}");
    assert!(m.contains("on | off | status"), "missing line = {m:?}");

    let unknown = dispatch(&ctx, "/auto wiggle").await.unwrap().unwrap();
    let OutputLine::Warn(u) = &unknown.lines[0] else {
        panic!("expected Warn, got {:?}", unknown.lines);
    };
    assert!(u.contains("wiggle"), "unknown line = {u:?}");

    // Neither path should have touched the adapter.
    assert_eq!(src.current(), AutoMode::Off);
}

#[tokio::test]
async fn headless_start_then_status_reports_running_with_socket() {
    let src = Arc::new(MockSupervisorSource::new(false));
    let ctx = ctx_with_supervisor(src.clone());

    let started = dispatch(&ctx, "/headless start").await.unwrap().unwrap();
    assert!(matches!(started.friction, Some(FrictionDecision::Proceed)));
    let OutputLine::Command(s) = &started.lines[0] else {
        panic!("expected Command, got {:?}", started.lines);
    };
    assert!(s.contains("start"), "line = {s:?}");
    assert!(s.contains("running"), "line = {s:?}");
    assert!(s.contains("changed"), "line = {s:?}");
    assert!(s.contains("socket=<operator-socket>"), "line = {s:?}");
    assert!(src.is_running());

    let status = dispatch(&ctx, "/headless status").await.unwrap().unwrap();
    let OutputLine::Command(line) = &status.lines[0] else {
        panic!("expected Command, got {:?}", status.lines);
    };
    assert!(line.contains("status"), "line = {line:?}");
    assert!(line.contains("running"), "line = {line:?}");
    assert!(
        !line.contains("changed"),
        "status must not claim `changed`, got: {line:?}"
    );
}

#[tokio::test]
async fn headless_without_adapter_surfaces_unavailable_alert() {
    let ctx = ctx_at(Label::Steady);
    let out = dispatch(&ctx, "/headless start").await.unwrap().unwrap();
    let OutputLine::Alert(line) = &out.lines[0] else {
        panic!("expected Alert, got {:?}", out.lines);
    };
    assert!(line.contains("unavailable"), "line = {line:?}");
}

#[tokio::test]
async fn headless_refused_surfaces_warn_not_alert() {
    // A supervisor adapter that returns `Refused` — e.g.
    // asked to stop while already stopping. The dispatcher
    // must render this as a warn (understood, not honored)
    // rather than an alert, so the log colour reflects "not
    // a crisis, just a no-op."
    struct AlwaysRefuse;
    impl SupervisorSource for AlwaysRefuse {
        fn act(&self, _action: SupervisorAction) -> Result<SupervisorReply, SupervisorError> {
            Err(SupervisorError::Refused("already stopping".into()))
        }
        fn tear_down_socket(&self) -> Result<bool, SupervisorError> {
            Ok(false)
        }
    }
    let ctx = ctx_with_supervisor(Arc::new(AlwaysRefuse));
    let out = dispatch(&ctx, "/headless stop").await.unwrap().unwrap();
    let OutputLine::Warn(line) = &out.lines[0] else {
        panic!("expected Warn, got {:?}", out.lines);
    };
    assert!(line.contains("refused"), "line = {line:?}");
    assert!(line.contains("already stopping"), "line = {line:?}");
}

// ---- 4. /kill compound -----------------------------------------------------

#[tokio::test]
async fn kill_with_running_supervisor_tears_down_socket_and_tags_line() {
    // Compound behavior — `/kill` with an attached, running
    // daemon tears down the listener socket as part of the
    // same call and tags the confirmation line so the operator
    // sees both effects in one breadcrumb.
    let src = Arc::new(MockSupervisorSource::new(true));
    let ctx = ctx_with_supervisor(src.clone());
    assert!(src.is_running());

    let out = dispatch(&ctx, "/kill").await.unwrap().unwrap();
    assert_eq!(out.risk, Some(RiskDirection::Reduces));
    assert!(matches!(out.friction, Some(FrictionDecision::Proceed)));

    assert!(
        out.lines.iter().any(|line| matches!(
            line,
            OutputLine::Alert(s) if s.contains("engine client unavailable")
        )),
        "live kill honesty line missing: {:?}",
        out.lines,
    );
    assert!(
        out.lines.iter().any(|line| matches!(
            line,
            OutputLine::Alert(s) if s.contains("headless supervisor") && s.contains("operator-local socket")
        )),
        "headless tear-down line missing: {:?}",
        out.lines,
    );

    assert!(!src.is_running(), "daemon must be stopped");
    assert!(src.socket_torn_down(), "socket must have been torn down");
}

#[tokio::test]
async fn kill_without_supervisor_surfaces_missing_engine_client() {
    // Regression lock — an invocation with no daemon adapter
    // attached must emit the *exact* pre-M2 wording so
    // scripts / snapshots that grep on the phrase keep
    // working. A new suffix here would be a contract drift.
    let ctx = ctx_at(Label::Steady);
    let out = dispatch(&ctx, "/kill").await.unwrap().unwrap();
    let OutputLine::Alert(line) = &out.lines[0] else {
        panic!("expected Alert, got {:?}", out.lines);
    };
    assert!(
        line.contains("engine client unavailable"),
        "line = {line:?}"
    );
    assert!(line.contains("live kill not posted"), "line = {line:?}");
}

#[tokio::test]
async fn kill_with_stopped_supervisor_does_not_tag_line() {
    // Attached supervisor, but the daemon is already stopped.
    // `/kill` must not tag the line — there is nothing to tear
    // down, and a tag would be a lie about what happened.
    let src = Arc::new(MockSupervisorSource::new(false));
    let ctx = ctx_with_supervisor(src.clone());

    let out = dispatch(&ctx, "/kill").await.unwrap().unwrap();
    let OutputLine::Alert(line) = &out.lines[0] else {
        panic!("expected Alert, got {:?}", out.lines);
    };
    assert!(
        !line.contains("headless"),
        "line must not tag when daemon was already stopped, got: {line:?}"
    );
    assert!(
        line.contains("engine client unavailable"),
        "line = {line:?}"
    );
}

// ---- misc contract pins ----------------------------------------------------

#[test]
fn auto_request_enum_covers_every_actionable_variant() {
    // The `AutoRequest` enum is the adapter-facing surface;
    // it must cover exactly the three actionable verbs and
    // nothing else. A drift here (adding `Pause` without a
    // parser arm, or conflating `On`/`Off`) would surface on
    // the adapter side with no parser witness — this test
    // is the witness.
    let all = [AutoRequest::On, AutoRequest::Off, AutoRequest::Status];
    assert_eq!(all.len(), 3);
    // Round-trip the three through the mock so any future
    // variant forces a compile-break on the `match` below.
    let src = MockAutoSource::new(AutoMode::Off);
    for r in all {
        let reply = src.act(r).unwrap();
        match r {
            AutoRequest::On => assert_eq!(reply.mode, AutoMode::On),
            AutoRequest::Off => assert_eq!(reply.mode, AutoMode::Off),
            AutoRequest::Status => { /* mode reflects prior state */ }
        }
    }
}

#[test]
fn supervisor_action_enum_covers_every_actionable_variant() {
    // Parallel pin for `SupervisorAction`. The compile-break
    // on a future variant lands here.
    let src = MockSupervisorSource::new(false);
    for action in [
        SupervisorAction::Start,
        SupervisorAction::Status,
        SupervisorAction::Stop,
    ] {
        let reply = src.act(action).unwrap();
        match action {
            SupervisorAction::Start => assert_eq!(reply.state, SupervisorState::Running),
            SupervisorAction::Stop => assert_eq!(reply.state, SupervisorState::Stopped),
            SupervisorAction::Status => { /* state depends on prior action */ }
        }
    }
}
