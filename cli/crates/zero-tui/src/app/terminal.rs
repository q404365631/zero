//! RAII wrapper around the crossterm terminal — enters raw mode +
//! alternate screen on construction, restores on drop.
//!
//! A panic hook is installed so a panic anywhere in the app still
//! leaves the operator's terminal usable. The hook runs before the
//! default panic handler, so the backtrace is still visible.

use std::io::{self, Stdout};

use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

pub type Tty = Terminal<CrosstermBackend<Stdout>>;

/// Owns the terminal for the duration of the app. Call
/// [`TerminalGuard::init`] to enter the app mode; the guard's
/// `Drop` impl restores on exit.
pub struct TerminalGuard {
    pub tty: Tty,
}

impl std::fmt::Debug for TerminalGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TerminalGuard").finish_non_exhaustive()
    }
}

impl TerminalGuard {
    /// Enter raw mode + alternate screen and install the panic
    /// restore hook. Safe to call once per process.
    ///
    /// # Errors
    /// Returns any error from `enable_raw_mode` or screen switch.
    pub fn init() -> io::Result<Self> {
        install_panic_hook();
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let tty = Terminal::new(backend)?;
        Ok(Self { tty })
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = restore();
    }
}

/// Restore cooked mode + primary screen. Idempotent.
fn restore() -> io::Result<()> {
    let mut stdout = io::stdout();
    execute!(stdout, LeaveAlternateScreen)?;
    disable_raw_mode()?;
    Ok(())
}

/// Panic hook that flushes the terminal back to a usable state
/// before the default handler prints the trace. Installed once.
fn install_panic_hook() {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let original = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let _ = restore();
            original(info);
        }));
    });
}
