//! Input translation — turn crossterm key events into app state
//! mutations. Isolated from the event loop so unit tests can drive
//! keystrokes directly against an `AppState`.

use std::time::Instant;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::mode::Mode;
use crate::app::state::{ActiveOverlay, AppState};

pub fn handle_key(state: &mut AppState, key: KeyEvent) {
    handle_key_inner(state, key);
    // Picker mirrors the prompt — rebuild after any input event so
    // the highlighted entry and list stay in sync with what the
    // operator is typing. Cheap: the catalog is 14 entries.
    state.refresh_picker();
}

fn handle_key_inner(state: &mut AppState, key: KeyEvent) {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let shift = key.modifiers.contains(KeyModifiers::SHIFT);

    // Ctrl+C, Ctrl+D: exit. No confirmation — this is a terminal,
    // not a dialog box. Risk-reducing exit is always instant per
    // the risk asymmetry (ADR-014). Also exits the overlay path,
    // since a modal that can trap a quit would violate the
    // "risk-reducing actions are frictionless" rule.
    if ctrl && matches!(key.code, KeyCode::Char('c' | 'd')) {
        state.should_quit = true;
        return;
    }

    // Modal overlays get priority over the prompt. Each variant
    // has its own affordance:
    //
    // * `State` — information-only. Any key dismisses. The gate
    //   contract the user is used to.
    // * `FrictionPause` — gated. Esc cancels (drops the pending
    //   command). At L1 the pause alone is the gate, so any other
    //   key is ignored. At L2+ typed characters feed the confirm
    //   buffer once the mandatory pause has elapsed; Backspace
    //   edits; Enter is a no-op (completion is detected by the
    //   event loop via `AppState::take_confirmed_friction_command`).
    if state.overlay.is_some() {
        match state.overlay.as_mut() {
            Some(ActiveOverlay::State | ActiveOverlay::Verdict(_) | ActiveOverlay::Risk { .. }) => {
                // Data overlays — same dismissal contract as the
                // state overview: any key closes. The verdict
                // overlay is read-only too; the `Evaluation`
                // payload is ephemeral (not kept after dismiss),
                // matching how the state overlay re-reads the
                // mirror every render. The Risk overlay (M2 §4)
                // also dismisses on any key; `dismiss_overlay`
                // records the dismissal timestamp so the auto-
                // open hook can enforce the 60 s cooldown.
                state.dismiss_overlay();
                return;
            }
            Some(ActiveOverlay::FrictionPause(fp)) => {
                if matches!(key.code, KeyCode::Esc) {
                    state.dismiss_overlay();
                    return;
                }
                // Ctrl+anything else at the friction overlay is
                // swallowed — the overlay is modal and we do not
                // want stray mode switches / splits through a
                // pending gate. Ctrl+C already exited above.
                if ctrl {
                    return;
                }
                let now = Instant::now();
                match key.code {
                    KeyCode::Char(c) if !ctrl => fp.push_char(c, now),
                    KeyCode::Backspace => fp.pop_char(now),
                    _ => {}
                }
                return;
            }
            None => unreachable!("overlay.is_some() established above"),
        }
    }

    // Mode switchers: Ctrl+0..4.
    if ctrl
        && let KeyCode::Char(c) = key.code
        && let Some(d) = c.to_digit(10)
        && let Ok(d) = u8::try_from(d)
        && let Some(mode) = Mode::from_digit(d)
    {
        state.mode = mode;
        return;
    }

    // Ctrl+R toggles screen-reader mode. Log a single system row
    // so the operator has a visible confirmation; the row itself
    // renders through the new mode so it also serves as a smoke
    // test of the alternate path.
    if ctrl && matches!(key.code, KeyCode::Char('r')) {
        let on = state.toggle_screen_reader();
        state.push_system(if on {
            "[system] screen-reader mode on (Ctrl+R to toggle)"
        } else {
            "[system] screen-reader mode off (Ctrl+R to toggle)"
        });
        return;
    }

    // Alt+] toggles the live-stream pane. Using the Alt modifier
    // (rather than a bare `]`) avoids clashing with operators
    // typing `]` into the prompt — the trading log has enough
    // hazards without a keystroke ambiguity. A confirmation row
    // in the conversation log is important the first few times
    // so the operator knows the toggle fired even when the pane
    // had nothing to render.
    if key.modifiers.contains(KeyModifiers::ALT) && matches!(key.code, KeyCode::Char(']')) {
        let on = state.toggle_live_stream();
        state.push_system(if on {
            "[system] live-stream pane on (Alt+] to toggle)"
        } else {
            "[system] live-stream pane off (Alt+] to toggle)"
        });
        return;
    }

    // Scrollback: PageUp/PageDown walk one "page" (12 rows is
    // enough to be useful on a short terminal and not too much on
    // a tall one). Ctrl+PageUp / Ctrl+PageDown jump to top/bottom
    // — the bottom jump re-attaches to the live tail.
    if handle_scrollback(state, key.code, ctrl) {
        return;
    }

    // Default: prompt editing, with picker-aware Up/Down/Tab.
    handle_prompt_edit(state, key.code, ctrl, shift);
}

/// Prompt-editing branch of [`handle_key_inner`]. Picker-aware:
/// `Up/Down` move selection inside an active picker, `Tab`
/// completes the highlighted entry, `Enter` submits (and
/// `Shift+Enter` inserts a newline).
fn handle_prompt_edit(state: &mut AppState, code: KeyCode, ctrl: bool, shift: bool) {
    match code {
        KeyCode::Enter => {
            if shift {
                state.prompt.insert_newline();
            } else {
                state.submit_prompt();
            }
        }
        KeyCode::Tab => {
            if let Some(picker) = state.picker.as_ref()
                && let Some(text) = picker.completion_text()
            {
                state.prompt.replace_all(&text);
            }
        }
        KeyCode::Up => {
            // Routing: picker > multi-row nav > history recall.
            if let Some(picker) = state.picker.as_mut() {
                picker.select_prev();
            } else if state.prompt.cursor_on_first_row() {
                state.prompt.recall_prev();
            } else {
                state.prompt.move_up();
            }
        }
        KeyCode::Down => {
            if let Some(picker) = state.picker.as_mut() {
                picker.select_next();
            } else if state.prompt.cursor_on_last_row() {
                state.prompt.recall_next();
            } else {
                state.prompt.move_down();
            }
        }
        KeyCode::Backspace => state.prompt.backspace(),
        KeyCode::Delete => state.prompt.delete(),
        KeyCode::Left => state.prompt.move_left(),
        KeyCode::Right => state.prompt.move_right(),
        KeyCode::Home => state.prompt.move_home(),
        KeyCode::End => state.prompt.move_end(),
        KeyCode::Esc => state.prompt.clear(),
        KeyCode::Char(c) => {
            // Strip Ctrl+Char where we didn't handle it above —
            // we don't want spurious chars leaking into the
            // prompt.
            if !ctrl {
                state.prompt.insert(c);
            }
        }
        _ => {}
    }
}

/// Scrollback step size for PageUp/PageDown. A dozen rows is a
/// comfortable scan speed on a 24-row pane and barely shifts on a
/// 60-row one — in both cases the operator sees context, not a
/// full flip.
const SCROLL_PAGE_ROWS: u16 = 12;

/// Scrollback key handler split out of [`handle_key_inner`] to
/// keep the dispatcher under the clippy line budget. Returns
/// `true` when the key was handled here.
fn handle_scrollback(state: &mut AppState, code: KeyCode, ctrl: bool) -> bool {
    match code {
        KeyCode::PageUp => {
            if ctrl {
                // Jump toward oldest. `u16::MAX` is effectively
                // unbounded relative to a 2048-cap log.
                state.scroll_log_up(u16::MAX);
            } else {
                state.scroll_log_up(SCROLL_PAGE_ROWS);
            }
            true
        }
        KeyCode::PageDown => {
            if ctrl {
                state.scroll_log_to_bottom();
            } else {
                state.scroll_log_down(SCROLL_PAGE_ROWS);
            }
            true
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::handle_key;
    use crate::app::mode::Mode;
    use crate::app::state::{ActiveOverlay, AppState};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use zero_engine_client::EngineState;

    fn mk() -> AppState {
        AppState::new(EngineState::shared())
    }

    #[test]
    fn typing_appends_to_prompt() {
        let mut s = mk();
        for c in "hi".chars() {
            handle_key(&mut s, KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
        }
        assert_eq!(s.prompt.as_string(), "hi");
    }

    #[test]
    fn enter_submits_and_clears() {
        let mut s = mk();
        for c in "/help".chars() {
            handle_key(&mut s, KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
        }
        handle_key(&mut s, KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(s.prompt.is_empty());
    }

    #[test]
    fn ctrl_c_quits() {
        let mut s = mk();
        handle_key(
            &mut s,
            KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
        );
        assert!(s.should_quit);
    }

    #[test]
    fn ctrl_digit_switches_mode() {
        let mut s = mk();
        handle_key(
            &mut s,
            KeyEvent::new(KeyCode::Char('2'), KeyModifiers::CONTROL),
        );
        assert_eq!(s.mode, Mode::Positions);
        handle_key(
            &mut s,
            KeyEvent::new(KeyCode::Char('4'), KeyModifiers::CONTROL),
        );
        assert_eq!(s.mode, Mode::Heat);
        handle_key(
            &mut s,
            KeyEvent::new(KeyCode::Char('0'), KeyModifiers::CONTROL),
        );
        assert_eq!(s.mode, Mode::Conversation);
    }

    #[test]
    fn overlay_dismisses_on_any_key() {
        use crate::app::state::ActiveOverlay;
        let mut s = mk();
        s.overlay = Some(ActiveOverlay::State);
        handle_key(
            &mut s,
            KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE),
        );
        assert!(s.overlay.is_none());
        assert!(
            s.prompt.is_empty(),
            "key that closes the overlay must not leak into prompt"
        );
    }

    #[test]
    fn overlay_does_not_trap_ctrl_c() {
        use crate::app::state::ActiveOverlay;
        let mut s = mk();
        s.overlay = Some(ActiveOverlay::State);
        handle_key(
            &mut s,
            KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
        );
        assert!(s.should_quit, "Ctrl+C must exit even through an overlay");
    }

    #[test]
    fn verdict_overlay_dismisses_on_any_key() {
        use crate::app::state::ActiveOverlay;
        use zero_engine_client::Evaluation;
        let mut s = mk();
        s.overlay = Some(ActiveOverlay::Verdict(Box::new(Evaluation {
            coin: Some("BTC".into()),
            direction: Some("LONG".into()),
            ..Default::default()
        })));
        handle_key(
            &mut s,
            KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE),
        );
        assert!(s.overlay.is_none(), "verdict overlay must dismiss");
        assert!(
            s.prompt.is_empty(),
            "dismissing keystroke must not leak into prompt"
        );
    }

    #[test]
    fn verdict_overlay_survives_ctrl_c_exit() {
        use crate::app::state::ActiveOverlay;
        use zero_engine_client::Evaluation;
        let mut s = mk();
        s.overlay = Some(ActiveOverlay::Verdict(Box::<Evaluation>::default()));
        handle_key(
            &mut s,
            KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
        );
        assert!(
            s.should_quit,
            "Ctrl+C must still exit through a verdict overlay"
        );
    }

    #[test]
    fn overlay_dismiss_swallows_ctrl_digit_mode_switch() {
        use crate::app::state::ActiveOverlay;
        let mut s = mk();
        s.mode = Mode::Conversation;
        s.overlay = Some(ActiveOverlay::State);
        handle_key(
            &mut s,
            KeyEvent::new(KeyCode::Char('2'), KeyModifiers::CONTROL),
        );
        assert!(s.overlay.is_none(), "overlay should be dismissed");
        assert_eq!(
            s.mode,
            Mode::Conversation,
            "the dismissing keystroke must not double-fire as a mode switch"
        );
    }

    #[test]
    fn friction_overlay_esc_cancels_and_drops_command() {
        use crate::app::state::{ActiveOverlay, FrictionPause};
        use std::time::{Duration, Instant};
        use zero_commands::Command;
        use zero_operator_state::friction::FrictionLevel;
        let mut s = mk();
        s.overlay = Some(ActiveOverlay::FrictionPause(FrictionPause {
            command: Command::Execute,
            level: FrictionLevel::L1,
            started_at: Instant::now(),
            pause: Duration::from_secs(3),
            confirm_word: None,
            confirm_input: String::new(),
        }));
        handle_key(&mut s, KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(s.overlay.is_none(), "Esc at friction overlay cancels it");
    }

    #[test]
    fn friction_overlay_l1_ignores_typed_keys() {
        use crate::app::state::{ActiveOverlay, FrictionPause};
        use std::time::{Duration, Instant};
        use zero_commands::Command;
        use zero_operator_state::friction::FrictionLevel;
        let mut s = mk();
        s.overlay = Some(ActiveOverlay::FrictionPause(FrictionPause {
            command: Command::Execute,
            level: FrictionLevel::L1,
            started_at: Instant::now(),
            pause: Duration::from_secs(3),
            confirm_word: None,
            confirm_input: String::new(),
        }));
        handle_key(
            &mut s,
            KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE),
        );
        assert!(
            s.overlay.is_some(),
            "typed chars at L1 must not dismiss the overlay"
        );
        assert!(
            s.prompt.is_empty(),
            "typed chars at L1 must not leak into prompt"
        );
    }

    #[test]
    fn friction_overlay_l2_does_not_accept_typing_during_pause() {
        use crate::app::state::{ActiveOverlay, FrictionPause};
        use std::time::{Duration, Instant};
        use zero_commands::Command;
        use zero_operator_state::friction::FrictionLevel;
        let mut s = mk();
        s.overlay = Some(ActiveOverlay::FrictionPause(FrictionPause {
            command: Command::Execute,
            level: FrictionLevel::L2,
            started_at: Instant::now(),
            pause: Duration::from_secs(10),
            confirm_word: Some("execute".into()),
            confirm_input: String::new(),
        }));
        for c in "execute".chars() {
            handle_key(&mut s, KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
        }
        if let Some(ActiveOverlay::FrictionPause(fp)) = &s.overlay {
            assert!(
                fp.confirm_input.is_empty(),
                "mandatory pause must reject typing; got {:?}",
                fp.confirm_input
            );
        } else {
            panic!("overlay was dismissed unexpectedly");
        }
    }

    #[test]
    fn friction_overlay_l2_accepts_typing_after_pause() {
        use crate::app::state::{ActiveOverlay, FrictionPause};
        use std::time::{Duration, Instant};
        use zero_commands::Command;
        use zero_operator_state::friction::FrictionLevel;
        let mut s = mk();
        s.overlay = Some(ActiveOverlay::FrictionPause(FrictionPause {
            command: Command::Execute,
            level: FrictionLevel::L2,
            // started_at in the past so the pause is already done
            // at the time the event loop fires the next key event.
            started_at: Instant::now()
                .checked_sub(Duration::from_secs(11))
                .expect("monotonic Instant supports 11s subtraction"),
            pause: Duration::from_secs(10),
            confirm_word: Some("execute".into()),
            confirm_input: String::new(),
        }));
        for c in "exec".chars() {
            handle_key(&mut s, KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
        }
        if let Some(ActiveOverlay::FrictionPause(fp)) = &s.overlay {
            assert_eq!(fp.confirm_input, "exec");
        } else {
            panic!("overlay dismissed unexpectedly");
        }
        // Backspace edits the confirm buffer, not the prompt.
        handle_key(
            &mut s,
            KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE),
        );
        if let Some(ActiveOverlay::FrictionPause(fp)) = &s.overlay {
            assert_eq!(fp.confirm_input, "exe");
        }
    }

    #[test]
    fn shift_enter_inserts_newline_instead_of_submitting() {
        let mut s = mk();
        for c in "abc".chars() {
            handle_key(&mut s, KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
        }
        handle_key(&mut s, KeyEvent::new(KeyCode::Enter, KeyModifiers::SHIFT));
        for c in "def".chars() {
            handle_key(&mut s, KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
        }
        assert_eq!(s.prompt.as_string(), "abc\ndef");
        assert!(s.pending_input.is_none(), "Shift+Enter must not submit");
        // Plain Enter now submits the joined buffer.
        handle_key(&mut s, KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(s.pending_input.as_deref(), Some("abc\ndef"));
    }

    #[test]
    fn up_recalls_previous_history_when_on_first_row() {
        let mut s = mk();
        for c in "/status".chars() {
            handle_key(&mut s, KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
        }
        handle_key(&mut s, KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        // pending_input is drained by the event loop in prod; clear it
        // here to reset submission state.
        s.pending_input = None;
        for c in "/risk".chars() {
            handle_key(&mut s, KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
        }
        handle_key(&mut s, KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        s.pending_input = None;
        // Buffer is empty; Up should recall the newest entry.
        handle_key(&mut s, KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
        // After recall, the buffer starts with `/` so the picker
        // is active — Up now navigates the picker, not history.
        assert_eq!(s.prompt.as_string(), "/risk");
    }

    #[test]
    fn up_navigates_picker_when_active() {
        let mut s = mk();
        for c in "/".chars() {
            handle_key(&mut s, KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
        }
        assert!(s.picker.is_some(), "typing / must open the picker");
        let first_selected = s.picker.as_ref().unwrap().selected_index();
        handle_key(&mut s, KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        assert_ne!(
            s.picker.as_ref().unwrap().selected_index(),
            first_selected,
            "Down with active picker should move selection"
        );
    }

    #[test]
    fn tab_completes_selected_picker_entry() {
        let mut s = mk();
        for c in "/he".chars() {
            handle_key(&mut s, KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
        }
        handle_key(&mut s, KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        assert_eq!(s.prompt.as_string(), "/help ");
    }

    #[test]
    fn esc_clears_prompt_and_picker_together() {
        let mut s = mk();
        for c in "/h".chars() {
            handle_key(&mut s, KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
        }
        assert!(s.picker.is_some());
        handle_key(&mut s, KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(s.prompt.as_string(), "");
        assert!(
            s.picker.is_none(),
            "clearing the buffer must also dismiss the ambient picker"
        );
    }

    #[test]
    fn pageup_detaches_pagedown_reattaches_scrollback() {
        let mut s = mk();
        for i in 0..30 {
            s.push_system(format!("row {i}"));
        }
        assert_eq!(s.log_scroll, 0);
        handle_key(&mut s, KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE));
        assert!(s.log_scroll > 0, "PageUp must detach the viewport");
        handle_key(
            &mut s,
            KeyEvent::new(KeyCode::PageDown, KeyModifiers::CONTROL),
        );
        assert_eq!(s.log_scroll, 0, "Ctrl+PageDown re-attaches to bottom");
    }

    #[test]
    fn ctrl_r_toggles_screen_reader_mode() {
        let mut s = mk();
        assert!(!s.screen_reader);
        handle_key(
            &mut s,
            KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL),
        );
        assert!(s.screen_reader);
        handle_key(
            &mut s,
            KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL),
        );
        assert!(!s.screen_reader);
    }

    #[test]
    fn submit_detaches_scroll_if_scrolled_up() {
        let mut s = mk();
        for i in 0..30 {
            s.push_system(format!("row {i}"));
        }
        s.scroll_log_up(10);
        assert_eq!(s.log_scroll, 10);
        for c in "/status".chars() {
            handle_key(&mut s, KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
        }
        handle_key(&mut s, KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(
            s.log_scroll, 0,
            "submit should re-attach to bottom so command output is visible"
        );
    }

    #[test]
    fn alt_right_bracket_toggles_live_stream_pane() {
        let mut s = mk();
        assert!(!s.live_stream_visible);
        handle_key(&mut s, KeyEvent::new(KeyCode::Char(']'), KeyModifiers::ALT));
        assert!(
            s.live_stream_visible,
            "Alt+] should turn the pane on from the hidden default"
        );
        handle_key(&mut s, KeyEvent::new(KeyCode::Char(']'), KeyModifiers::ALT));
        assert!(
            !s.live_stream_visible,
            "second Alt+] should turn the pane off again"
        );
    }

    #[test]
    fn bare_right_bracket_is_typed_into_prompt_not_a_toggle() {
        // The toggle is deliberately bound to Alt+] — a bare `]`
        // in the prompt must flow through to the buffer. This is
        // the conflict we chose the modifier to avoid.
        let mut s = mk();
        handle_key(
            &mut s,
            KeyEvent::new(KeyCode::Char(']'), KeyModifiers::NONE),
        );
        assert!(!s.live_stream_visible, "bare `]` must not toggle the pane");
        assert_eq!(
            s.prompt.as_string(),
            "]",
            "bare `]` must land in the prompt buffer"
        );
    }

    #[test]
    fn alt_right_bracket_inside_overlay_is_swallowed() {
        // Any modal overlay takes priority: Alt+] must not sneak
        // through and toggle the pane while the operator is
        // reading a state/verdict/friction overlay.
        let mut s = mk();
        s.overlay = Some(ActiveOverlay::State);
        handle_key(&mut s, KeyEvent::new(KeyCode::Char(']'), KeyModifiers::ALT));
        assert!(
            !s.live_stream_visible,
            "overlays swallow keys — toggle must not fire"
        );
    }

    #[test]
    fn ctrl_digit_five_is_unbound() {
        let mut s = mk();
        s.mode = Mode::Decisions;
        handle_key(
            &mut s,
            KeyEvent::new(KeyCode::Char('5'), KeyModifiers::CONTROL),
        );
        assert_eq!(s.mode, Mode::Decisions, "Ctrl+5 must not change mode");
    }
}
