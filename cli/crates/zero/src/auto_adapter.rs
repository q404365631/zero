//! Real `AutoSource` adapter — talks to the engine's
//! `/auto/toggle` HTTP endpoint.
//!
//! The dispatcher's [`AutoSource`] trait is sync (the dispatcher
//! itself is sync so a `/kill`-adjacent path cannot deadlock
//! behind an async boundary). The [`HttpClient`] is async, so
//! this adapter bridges the two with `block_in_place` on the
//! ambient multi-threaded tokio runtime — the same pattern the
//! `HeadlessSupervisorAdapter` uses for the daemon socket.
//!
//! # Semantics
//!
//! - `act(On)` / `act(Off)` call
//!   [`HttpClient::post_auto_toggle`] under M2 §7's no-retry
//!   rule. A transient 5xx / timeout surfaces as
//!   `SupervisorError::Io(...)` immediately; the CLI never
//!   silently re-sends a composition request. The engine's
//!   returned `simulated` flag is carried through to the
//!   dispatcher via the `AutoReply` — but the dispatcher does
//!   not surface it directly today, so for now we drop it
//!   (a future `/auto` line carrying the `(simulated)` tag
//!   would read this).
//!
//! - `act(Status)` returns an honest "unavailable" error
//!   today. The engine has no read-only `GET /auto` surface
//!   in M2; `POST /auto/toggle` is the only way to observe
//!   state, but it also mutates, which would make `Status`
//!   unsafe. The `zero auto` subcommand surfaces this as a
//!   clear hint ("use `zero auto on|off` to set the posture
//!   explicitly"); silence here would be the exact
//!   dishonesty M2 §8 guards against.
//!
//! When the engine grows a dedicated read-only auto endpoint
//! (tracked; not in M2 scope), `Status` flips to a GET under
//! the ordinary retry rule (reading a posture is not a
//! composition change) and the hint stops firing.

use std::sync::Arc;

use tokio::runtime::Handle;
use zero_commands::{AutoMode, AutoReply, AutoRequest, AutoSource, SupervisorError};
use zero_engine_client::HttpClient;

/// The real `AutoSource`. Cheap to clone — holds an `Arc` to
/// the `HttpClient` + the runtime handle.
#[derive(Debug, Clone)]
pub struct HttpAutoSource {
    http: HttpClient,
    handle: Handle,
}

impl HttpAutoSource {
    /// Build an adapter from a live HTTP client + the runtime
    /// handle the adapter should block on. In the `zero` binary
    /// we pass `Handle::current()` captured from the
    /// `#[tokio::main]` entrypoint.
    /// Construct an `Arc<dyn AutoSource>` suitable for
    /// handing to `DispatchContext::with_auto`.
    #[must_use]
    pub fn shared(http: HttpClient, handle: Handle) -> Arc<dyn AutoSource> {
        Arc::new(Self { http, handle })
    }
}

impl AutoSource for HttpAutoSource {
    fn act(&self, action: AutoRequest) -> Result<AutoReply, SupervisorError> {
        match action {
            AutoRequest::On | AutoRequest::Off => {
                let enabled = matches!(action, AutoRequest::On);
                // `block_in_place` + `block_on` is the same
                // sync-to-async bridge the supervisor adapter
                // uses. Safe on the multi-threaded runtime.
                let reply = tokio::task::block_in_place(|| {
                    self.handle.block_on(self.http.post_auto_toggle(enabled))
                });
                match reply {
                    Ok(resp) => {
                        let new_mode = match resp.state {
                            zero_engine_client::AutoState::On => AutoMode::On,
                            zero_engine_client::AutoState::Off => AutoMode::Off,
                        };
                        // We cannot tell whether this call
                        // *changed* the mode without a prior
                        // read — and there is no read surface.
                        // Reporting `changed = true` on every
                        // accepted toggle is the honest upper
                        // bound; the dispatcher's idempotent
                        // warn path is best-effort anyway
                        // without engine cooperation.
                        Ok(AutoReply {
                            mode: new_mode,
                            changed: true,
                        })
                    }
                    Err(e) => Err(SupervisorError::Io(format!(
                        "/auto/toggle: {e}"
                    ))),
                }
            }
            AutoRequest::Status => {
                // The engine exposes auto state only as a
                // side-effect of `POST /auto/toggle` (the M2
                // §7 surface). There is no `GET /auto` (or
                // equivalent read-only query) on the wire
                // today, and the snapshot the
                // `EngineStatePoller` pulls does not carry
                // an auto flag either. An honest "I don't
                // know" beats a fabricated "off".
                //
                // When the engine grows a dedicated
                // read-only auto endpoint (tracked; not in
                // M2 scope) this branch flips to a GET that
                // carries no retry rule — `Status` is not a
                // composition change.
                Err(SupervisorError::Io(
                    "auto status unavailable — engine exposes no read-only /auto endpoint; \
                     use `zero auto on|off` to set the posture explicitly"
                        .to_string(),
                ))
            }
        }
    }
}
