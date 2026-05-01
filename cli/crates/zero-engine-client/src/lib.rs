//! Unified HTTP + WebSocket + MCP client to the ZERO engine.
//!
//! The CLI never reads bus files. All engine state flows through this
//! crate. Every value carries freshness metadata (`Stat<T>`) and the
//! TUI refuses to render without it.

#![allow(clippy::module_name_repetitions)]

pub mod http;
pub mod models;
pub mod poll;
pub mod rate_budget;
pub mod stat;
pub mod state;
pub mod ws;

pub use http::{HttpClient, HttpError, Mode, RateLimitSource};
pub use models::{
    Approaching, ApproachingFeed, AutoState, AutoToggleRequest, AutoToggleResponse, Brief,
    ComponentCounts, ComponentHealth, Evaluation, EvaluationLayer, ExecuteRequest, ExecuteResponse,
    ExecuteSide, Health, HlRate, Position, Positions, Pulse, PulseEvent, Regime, Rejection,
    RejectionsFeed, Risk, RiskSummary, Root, V2Confidence, V2Market, V2Positions, V2Status,
    V2Today,
};
pub use poll::{BACKFILL_INTERVAL, EngineStatePoller, OperatorStatePoller, POLL_INTERVAL};
pub use rate_budget::{
    BudgetSnapshot, Clock, DEFAULT_CAPACITY, DEFAULT_REFILL_PER_SECOND, Exhausted, ManualClock,
    RateBudget, SystemClock, cost_of,
};
pub use stat::{Source, Stat};
pub use state::{ConnectionHealth, EngineState};
pub use ws::{
    EngineEvent, JitterMode, ReconnectConfig, WsError, WsSubscriber, apply_jitter, exp_backoff_cap,
};
