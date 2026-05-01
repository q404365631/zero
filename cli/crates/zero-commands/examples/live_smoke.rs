//! Live smoke check against the real engine.
//!
//! Runs the same `/status`, `/risk`, `/brief`, `/pos`, `/evaluate`
//! dispatch paths the TUI runs, pointed at the configured engine
//! URL + OS-keychain token. Prints the exact `OutputLine` stream
//! the TUI would render so a human can eyeball that live data
//! arrives without spinning up the whole TUI.
//!
//! Invocation:
//!
//! ```bash
//! ZERO_API_URL=https://api.getzero.dev \
//! ZERO_API_TOKEN=$(security find-generic-password -s dev.getzero.zero -a default -w) \
//!   cargo run --release -p zero-commands --example live_smoke
//! ```

use zero_commands::dispatch::{DispatchContext, dispatch};
use zero_engine_client::{EngineState, HttpClient};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let url = std::env::var("ZERO_API_URL").unwrap_or_else(|_| "https://api.getzero.dev".into());
    let token = std::env::var("ZERO_API_TOKEN").ok();
    let http = HttpClient::new(url::Url::parse(&url)?, token)?;
    let engine = EngineState::shared();
    let ctx = DispatchContext::new(Some(http), engine);

    for cmd in [
        "/status",
        "/risk",
        "/brief",
        "/pos",
        "/evaluate BTC",
        "/heat",
        // Endpoints that are commonly degraded on older engine
        // builds — we want the smoke to exercise the graceful-
        // degradation alerts (empty body, 404) and prove they
        // surface as real operator-visible errors rather than
        // em-dashes or raw HttpError::Display strings.
        "/regime",
        "/approaching",
    ] {
        println!("\n=== {cmd} ===");
        match dispatch(&ctx, cmd).await? {
            Some(out) => {
                for line in &out.lines {
                    println!("  {line:?}");
                }
                if let Some(ov) = &out.show_overlay {
                    println!("  [overlay: {ov:?}]");
                }
            }
            None => println!("  (no output)"),
        }
    }
    Ok(())
}
