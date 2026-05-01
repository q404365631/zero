//! Client-side dialer for the daemon.
//!
//! Used by the `zero-commands` real `SupervisorSource` adapter
//! and by the integration tests. A thin wrapper over a
//! `UnixStream`: one request per dial, line-delimited JSON,
//! hard-timed.
//!
//! # Why one-shot
//!
//! The CLI is the supervisor's client, not a long-lived
//! session. Every dial is a discrete intent ("arm", "disarm",
//! "kill", "how are you"). Reusing a connection would save a
//! handful of microseconds at the cost of having to reason
//! about a half-closed socket during a kill-switch round-trip.
//! We trade throughput for clarity here — the supervisor fields
//! human-scale request rates.

use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::time::timeout;

use crate::protocol::{Request, Response};

/// Total round-trip budget for a single dial. Generous: the
/// daemon's hottest path is a local socket write + in-memory
/// state flip + JSON serialize. If we don't have an answer in
/// 2 s the daemon is wedged and the operator should know.
pub const DIAL_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug, Error)]
pub enum ClientError {
    #[error("could not reach daemon at {path}: {source}")]
    Connect {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("daemon timed out after {timeout:?} at {path}")]
    Timeout { path: PathBuf, timeout: Duration },
    #[error("i/o while talking to daemon at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("daemon sent unparseable reply at {path}: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("daemon closed connection before replying at {path}")]
    Closed { path: PathBuf },
}

/// Dialer pinned at a socket path. Cheap to construct; the
/// actual `UnixStream` is opened per-request.
#[derive(Debug, Clone)]
pub struct Client {
    socket_path: PathBuf,
    timeout: Duration,
}

impl Client {
    #[must_use]
    pub fn new(socket_path: impl Into<PathBuf>) -> Self {
        Self {
            socket_path: socket_path.into(),
            timeout: DIAL_TIMEOUT,
        }
    }

    /// Override the per-request timeout. Tests rely on this to
    /// distinguish "daemon down" from "daemon slow".
    #[must_use]
    pub fn with_timeout(mut self, t: Duration) -> Self {
        self.timeout = t;
        self
    }

    #[must_use]
    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    /// Returns `true` if the socket file exists. Not a
    /// liveness check — a stale socket (daemon crashed
    /// without cleanup) will still return `true` and the next
    /// [`Self::send`] will surface the real failure mode.
    #[must_use]
    pub fn socket_exists(&self) -> bool {
        self.socket_path.exists()
    }

    /// Send a single request and wait for a single reply. All
    /// I/O is bounded by [`Self::with_timeout`].
    pub async fn send(&self, req: &Request) -> Result<Response, ClientError> {
        let fut = async {
            let stream = UnixStream::connect(&self.socket_path)
                .await
                .map_err(|source| ClientError::Connect {
                    path: self.socket_path.clone(),
                    source,
                })?;

            let (read_half, mut write_half) = stream.into_split();
            let mut line = serde_json::to_string(req).map_err(|source| ClientError::Parse {
                path: self.socket_path.clone(),
                source,
            })?;
            line.push('\n');

            write_half
                .write_all(line.as_bytes())
                .await
                .map_err(|source| ClientError::Io {
                    path: self.socket_path.clone(),
                    source,
                })?;
            write_half.flush().await.map_err(|source| ClientError::Io {
                path: self.socket_path.clone(),
                source,
            })?;

            let mut reader = BufReader::new(read_half);
            let mut response_line = String::new();
            let n = reader
                .read_line(&mut response_line)
                .await
                .map_err(|source| ClientError::Io {
                    path: self.socket_path.clone(),
                    source,
                })?;
            if n == 0 {
                return Err(ClientError::Closed {
                    path: self.socket_path.clone(),
                });
            }
            let trimmed = response_line.trim_end_matches(['\r', '\n']);
            serde_json::from_str::<Response>(trimmed).map_err(|source| ClientError::Parse {
                path: self.socket_path.clone(),
                source,
            })
        };

        match timeout(self.timeout, fut).await {
            Ok(r) => r,
            Err(_) => Err(ClientError::Timeout {
                path: self.socket_path.clone(),
                timeout: self.timeout,
            }),
        }
    }
}
