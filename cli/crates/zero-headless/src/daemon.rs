//! Daemon core — Unix-socket listener + request handler.
//!
//! Public entry point: [`Daemon::run`]. The binary wrapper in
//! `src/bin/zero-headlessd.rs` is thin glue: parse CLI flags,
//! spawn a [`Daemon`], wait on its shutdown handle.
//!
//! # Shutdown paths (all equivalent)
//!
//! 1. `Request::Kill` over the socket — set by `/kill` and the
//!    (M3) Telegram bot.
//! 2. `SIGTERM` from launchd / systemd.
//! 3. `Shutdown` handle held by the test harness.
//!
//! All three funnel through a single `shutdown_signal` channel;
//! the listener drops its accept loop on receive, open
//! connections drain their in-flight request, and then the
//! daemon exits with a zero status.
//!
//! # Engine probe
//!
//! Abstracted behind the [`EngineProbe`] trait so tests can
//! inject a deterministic stub without standing up an HTTP
//! server. Production uses [`HttpEngineProbe`] over
//! `zero_engine_client::HttpClient::health`.

use std::future::Future;
use std::io;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
#[cfg(unix)]
use tokio::signal::unix::{SignalKind, signal};
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};

use crate::protocol::{
    ActionKind, EngineHealth, PROTOCOL_VERSION, Request, Response, StatusReply, SupervisorState,
};
use crate::state::{PersistError, State};

/// Convenience alias for the engine probe's future type.
/// `Pin<Box<dyn Future>>` because `async fn` in dyn-compatible
/// traits is still a rough edge; manual boxing keeps the dyn
/// bound simple and the caller path explicit.
pub type ProbeFuture<'a> = Pin<Box<dyn Future<Output = Result<(), String>> + Send + 'a>>;

#[derive(Debug, Error)]
pub enum DaemonError {
    #[error("socket bind error at {path}: {source}")]
    Bind {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("socket i/o error at {path}: {source}")]
    Socket {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("state error: {0}")]
    Persist(#[from] PersistError),
    #[error("missing configuration: {0}")]
    MissingConfig(String),
}

/// Abstract engine reachability probe. The daemon calls
/// [`EngineProbe::probe`] on a schedule + on-demand before a
/// `/status` reply that would otherwise return stale data.
///
/// Manually boxed future (rather than `async fn` in trait)
/// keeps the trait object-safe without pulling in
/// `async_trait` just for one surface.
pub trait EngineProbe: Send + Sync + 'static {
    fn probe(&self) -> ProbeFuture<'_>;
}

/// Always-reachable probe — convenient for tests that care
/// about state + socket semantics and not about the probe
/// machinery.
#[derive(Debug, Default)]
pub struct AlwaysReachable;

impl EngineProbe for AlwaysReachable {
    fn probe(&self) -> ProbeFuture<'_> {
        Box::pin(async { Ok(()) })
    }
}

/// Always-unreachable probe — for testing the honest "engine
/// down" status path.
#[derive(Debug, Default)]
pub struct AlwaysUnreachable;

impl EngineProbe for AlwaysUnreachable {
    fn probe(&self) -> ProbeFuture<'_> {
        Box::pin(async { Err("probe fault-injected".into()) })
    }
}

/// Production probe: calls `GET /health` via
/// `zero_engine_client::HttpClient`. Kept outside the trait
/// so the daemon core stays free of an HTTP-client dependency
/// at the test boundary.
#[derive(Debug)]
pub struct HttpEngineProbe {
    client: zero_engine_client::HttpClient,
}

impl HttpEngineProbe {
    #[must_use]
    pub fn new(client: zero_engine_client::HttpClient) -> Self {
        Self { client }
    }
}

impl EngineProbe for HttpEngineProbe {
    fn probe(&self) -> ProbeFuture<'_> {
        Box::pin(async move {
            self.client
                .health()
                .await
                .map(|_| ())
                .map_err(|e| e.to_string())
        })
    }
}

/// Daemon configuration. Built by the binary's `main` from
/// CLI flags + defaults, and by tests via [`Config::for_test`].
#[derive(Debug, Clone)]
pub struct Config {
    pub socket_path: PathBuf,
    pub state_path: PathBuf,
    /// `false` → daemon refuses to start. The spec's
    /// "refuses-to-run-without-config" rule is enforced here.
    pub config_present: bool,
}

impl Config {
    /// Convenient test constructor: both paths rooted under
    /// `base`, config always "present". Production code builds
    /// this struct from real CLI flags.
    #[must_use]
    pub fn for_test(base: &Path) -> Self {
        Self {
            socket_path: crate::paths::socket_in(base),
            state_path: crate::paths::state_in(base),
            config_present: true,
        }
    }
}

/// Handle returned from [`Daemon::spawn`]. Holding it keeps the
/// daemon running; dropping it does not kill the daemon — tests
/// that want a guaranteed shutdown should call
/// [`Shutdown::shutdown`] and `.await` [`Shutdown::join`].
#[derive(Debug)]
pub struct Shutdown {
    tx: Option<oneshot::Sender<()>>,
    join: Option<JoinHandle<Result<(), DaemonError>>>,
    socket_path: PathBuf,
}

impl Shutdown {
    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    /// Signal graceful drain. Idempotent.
    pub fn shutdown(&mut self) {
        if let Some(tx) = self.tx.take() {
            let _ = tx.send(());
        }
    }

    /// Await daemon exit. Returns the daemon's final result.
    pub async fn join(mut self) -> Result<(), DaemonError> {
        self.shutdown();
        match self.join.take() {
            Some(handle) => match handle.await {
                Ok(res) => res,
                Err(join_err) => {
                    error!(?join_err, "daemon task panicked");
                    Ok(())
                }
            },
            None => Ok(()),
        }
    }
}

/// In-memory daemon instance. The fields are `Arc<Mutex<…>>`
/// so each accept-loop iteration can hand a handler task a
/// clone without colouring everything `Clone`.
pub struct Daemon {
    cfg: Config,
    state: Arc<Mutex<State>>,
    probe: Arc<dyn EngineProbe>,
    last_engine: Arc<Mutex<EngineHealth>>,
}

impl std::fmt::Debug for Daemon {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Daemon")
            .field("cfg", &self.cfg)
            .field("state", &*self.state.lock())
            .field("last_engine", &*self.last_engine.lock())
            .finish_non_exhaustive()
    }
}

impl Daemon {
    /// Build a daemon without spawning it. The caller picks an
    /// [`EngineProbe`] — production passes [`HttpEngineProbe`],
    /// tests pass [`AlwaysReachable`] / [`AlwaysUnreachable`].
    pub fn new(cfg: Config, probe: Arc<dyn EngineProbe>) -> Result<Self, DaemonError> {
        if !cfg.config_present {
            return Err(DaemonError::MissingConfig(
                "zero config missing — run `zero onboard` first".into(),
            ));
        }
        let state = State::load(&cfg.state_path)?;
        Ok(Self {
            cfg,
            state: Arc::new(Mutex::new(state)),
            probe,
            last_engine: Arc::new(Mutex::new(EngineHealth::Unknown)),
        })
    }

    /// Spawn the listener on a Tokio task. Returns a
    /// [`Shutdown`] handle; dropping it does *not* kill the
    /// daemon — call [`Shutdown::shutdown`] / `.join()`
    /// explicitly.
    pub fn spawn(self) -> Result<Shutdown, DaemonError> {
        // Best-effort cleanup of a stale socket file from a
        // prior crash. Failure here is not fatal — `bind` will
        // surface the real error if the path is genuinely
        // unusable.
        if self.cfg.socket_path.exists() {
            let _ = std::fs::remove_file(&self.cfg.socket_path);
        }
        if let Some(parent) = self.cfg.socket_path.parent()
            && let Err(source) = std::fs::create_dir_all(parent)
        {
            return Err(DaemonError::Bind {
                path: parent.to_path_buf(),
                source,
            });
        }

        let listener =
            UnixListener::bind(&self.cfg.socket_path).map_err(|source| DaemonError::Bind {
                path: self.cfg.socket_path.clone(),
                source,
            })?;

        info!(socket = %self.cfg.socket_path.display(), "zero-headlessd listening");

        let (tx, rx) = oneshot::channel::<()>();
        let socket_path = self.cfg.socket_path.clone();
        let join = tokio::spawn(self.run_accept_loop(listener, rx));
        Ok(Shutdown {
            tx: Some(tx),
            join: Some(join),
            socket_path,
        })
    }

    /// Convenience wrapper used by the binary: spawn + await
    /// the accept loop + SIGTERM in one call.
    pub async fn run(self) -> Result<(), DaemonError> {
        let mut handle = self.spawn()?;

        #[cfg(unix)]
        {
            let mut term =
                signal(SignalKind::terminate()).map_err(|source| DaemonError::Socket {
                    path: handle.socket_path.clone(),
                    source,
                })?;
            let mut int =
                signal(SignalKind::interrupt()).map_err(|source| DaemonError::Socket {
                    path: handle.socket_path.clone(),
                    source,
                })?;
            tokio::select! {
                _ = term.recv() => {
                    info!("SIGTERM received — draining");
                }
                _ = int.recv() => {
                    info!("SIGINT received — draining");
                }
            }
        }

        handle.shutdown();
        handle.join().await
    }

    async fn run_accept_loop(
        self,
        listener: UnixListener,
        mut shutdown_rx: oneshot::Receiver<()>,
    ) -> Result<(), DaemonError> {
        // Drain bookkeeping: each handler task holds a
        // `drain_permit` sender clone. When the accept loop
        // exits, we drop our own sender and wait for the
        // `drain_wait` channel to close — Tokio's standard
        // idiom for "let every in-flight task finish".
        let (drain_permit, mut drain_wait) = mpsc::channel::<()>(1);

        loop {
            tokio::select! {
                _ = &mut shutdown_rx => {
                    info!("shutdown requested — closing accept loop");
                    break;
                }
                accept = listener.accept() => {
                    match accept {
                        Ok((stream, _addr)) => {
                            let permit = drain_permit.clone();
                            let state = self.state.clone();
                            let state_path = self.cfg.state_path.clone();
                            let probe = self.probe.clone();
                            let last_engine = self.last_engine.clone();
                            tokio::spawn(async move {
                                if let Err(err) = handle_connection(
                                    stream,
                                    &state,
                                    &state_path,
                                    probe.as_ref(),
                                    &last_engine,
                                )
                                .await
                                {
                                    warn!(?err, "connection handler error");
                                }
                                drop(permit);
                            });
                        }
                        Err(source) => {
                            error!(?source, "accept failed");
                        }
                    }
                }
            }
        }

        drop(drain_permit);
        // Tokio's mpsc: `recv()` returns `None` once every
        // sender has been dropped — i.e. every handler task
        // has exited. That's our graceful-drain fence.
        while (drain_wait.recv().await).is_some() {}

        // Best-effort socket file cleanup so a restart doesn't
        // have to step over our rubble.
        let _ = std::fs::remove_file(&self.cfg.socket_path);
        Ok(())
    }
}

async fn handle_connection(
    stream: UnixStream,
    state: &Arc<Mutex<State>>,
    state_path: &Path,
    probe: &dyn EngineProbe,
    last_engine: &Arc<Mutex<EngineHealth>>,
) -> Result<(), DaemonError> {
    let (read_half, mut write_half) = stream.into_split();
    let mut reader = BufReader::new(read_half);
    let mut line = String::new();
    let n = reader
        .read_line(&mut line)
        .await
        .map_err(|source| DaemonError::Socket {
            path: state_path.to_path_buf(),
            source,
        })?;
    if n == 0 {
        return Ok(());
    }
    let trimmed = line.trim_end_matches(['\r', '\n']);

    let response = match serde_json::from_str::<Request>(trimmed) {
        Ok(req) => dispatch(req, state, state_path, probe, last_engine).await,
        Err(err) => Response::Error {
            reason: format!("malformed request: {err}"),
        },
    };

    let mut reply = serde_json::to_string(&response).unwrap_or_else(|_| {
        // Should be impossible — all `Response` variants are
        // `Serialize`. But a daemon that panics on a corrupted
        // reply would be worse than one that logs and moves on.
        "{\"kind\":\"error\",\"reason\":\"internal serialize failure\"}".into()
    });
    reply.push('\n');
    write_half
        .write_all(reply.as_bytes())
        .await
        .map_err(|source| DaemonError::Socket {
            path: state_path.to_path_buf(),
            source,
        })?;
    write_half
        .flush()
        .await
        .map_err(|source| DaemonError::Socket {
            path: state_path.to_path_buf(),
            source,
        })?;
    debug!(?response, "reply sent");
    Ok(())
}

async fn dispatch(
    req: Request,
    state: &Arc<Mutex<State>>,
    state_path: &Path,
    probe: &dyn EngineProbe,
    last_engine: &Arc<Mutex<EngineHealth>>,
) -> Response {
    match req {
        Request::Status => status_reply(state, probe, last_engine).await,
        Request::Start => mutate(state, state_path, |s| {
            s.set_intent(SupervisorState::On);
            s.push_action(ActionKind::Started, "start requested via socket");
        }),
        Request::Stop => mutate(state, state_path, |s| {
            s.set_intent(SupervisorState::Off);
            s.push_action(ActionKind::Stopped, "stop requested via socket");
        }),
        Request::Kill => {
            // Kill is the spec's never-silent path. Record it
            // durably *before* acking so a restarted daemon
            // can explain what happened.
            let _ = mutate(state, state_path, |s| {
                s.set_intent(SupervisorState::Off);
                s.push_action(ActionKind::Killed, "kill-switch fired via socket");
            });
            Response::Accepted {
                state: SupervisorState::Off,
                protocol_version: PROTOCOL_VERSION,
            }
            // Note: the accept loop is responsible for actually
            // exiting. It watches the shutdown oneshot; kill-
            // over-socket sets the state but the CLI's own
            // signal (or a follow-up SIGTERM from a top-level
            // kill-switch) drives the accept loop to exit.
            // This keeps the handler non-blocking and avoids
            // racing the write we just enqueued above.
        }
    }
}

fn mutate<F>(state: &Arc<Mutex<State>>, state_path: &Path, f: F) -> Response
where
    F: FnOnce(&mut State),
{
    let snap = {
        let mut guard = state.lock();
        f(&mut guard);
        guard.clone()
    };
    if let Err(err) = snap.save(state_path) {
        error!(?err, "state persist failed");
        return Response::Refused {
            reason: format!("state persist failed: {err}"),
        };
    }
    Response::Accepted {
        state: snap.intent,
        protocol_version: PROTOCOL_VERSION,
    }
}

async fn status_reply(
    state: &Arc<Mutex<State>>,
    probe: &dyn EngineProbe,
    last_engine: &Arc<Mutex<EngineHealth>>,
) -> Response {
    // Re-probe with a tight budget so `/headless status`
    // doesn't sit behind a flaky engine. If we time out, we
    // carry forward the last known health — still truthful.
    let probe_fut = probe.probe();
    let fresh = match tokio::time::timeout(Duration::from_millis(500), probe_fut).await {
        Ok(Ok(())) => Some(EngineHealth::Reachable {
            at: chrono::Utc::now(),
        }),
        Ok(Err(reason)) => Some(EngineHealth::Unreachable {
            at: chrono::Utc::now(),
            reason,
        }),
        Err(_) => None,
    };
    if let Some(ref h) = fresh {
        *last_engine.lock() = h.clone();
    }

    // Record a `Probed` action so `/headless status`'s "recent
    // decisions" shows the probe cadence, not just the explicit
    // start/stop/kill edges. We only note it when we actually
    // have a fresh result (a timed-out probe is already
    // surfaced via the stale engine health).
    if let Some(ref fresh_h) = fresh {
        let note = match fresh_h {
            EngineHealth::Reachable { .. } => "engine probe: reachable".to_string(),
            EngineHealth::Unreachable { reason, .. } => {
                format!("engine probe: unreachable ({reason})")
            }
            EngineHealth::Unknown => "engine probe: unknown".to_string(),
        };
        state.lock().push_action(ActionKind::Probed, note);
    }

    let (intent, latest_action) = {
        let s = state.lock();
        (s.intent, s.recent_actions.first().cloned())
    };
    let engine = last_engine.lock().clone();

    Response::Status(StatusReply {
        state: intent,
        engine,
        latest_action,
        protocol_version: PROTOCOL_VERSION,
    })
}
