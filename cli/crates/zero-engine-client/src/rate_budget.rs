//! Token-bucket rate budget sitting between [`crate::HttpClient`] and
//! the network.
//!
//! # Why a CLI-side bucket exists at all
//!
//! The engine has its own rate limiter; it will return 429 if the
//! operator hammers an endpoint. A CLI-side bucket is not a
//! redundancy — it is a **courtesy layer** that:
//!
//! 1. **Refuses visibly, not silently.** An operator who types
//!    `/status` twenty times in five seconds must read a refusal
//!    line naming the budget, not watch the prompt freeze for a
//!    retry loop. The freeze is the classic mystery stall that
//!    erodes trust in a CLI; a typed `rate: exhausted — retry in Ns`
//!    is fixable.
//! 2. **Protects the engine's own budget from blocking other
//!    operators.** The engine's bucket is per-operator; our CLI
//!    getting rate-limited by a local heuristic preserves headroom
//!    for Auto-mode + Telegram paths that operate without typing
//!    operators hammering them.
//! 3. **Is an anchor for the status bar segment.** `rate:N/M`
//!    paints the current bucket fill in the always-visible tier;
//!    this module is the source of truth the widget reads.
//!
//! # Determinism under test
//!
//! The bucket's time source is a `Clock` trait — production uses
//! `SystemClock` (thin wrapper over `std::time::Instant`), tests use
//! `ManualClock` and advance explicitly. Every budget assertion in
//! the test suite is wall-clock-free and therefore flake-free.
//!
//! # Thread safety
//!
//! `RateBudget` is `Arc<Inner>` where `Inner` holds a `parking_lot::
//! Mutex<State>`. The critical section is a handful of float math
//! ops; `parking_lot`'s uncontended lock is ~25 ns, well under the
//! network RTT the bucket guards. A sharded / atomic design would
//! buy nothing and cost legibility.
//!
//! # What the bucket is **not**
//!
//! - Not a global limiter. Multiple operators, multiple CLIs, even
//!   multiple `HttpClient`s in the same process each hold their
//!   own bucket. Cross-process coordination would require IPC that
//!   buys less than it costs at M2 scale.
//! - Not persistent. A CLI restart gets a fresh full bucket. An
//!   operator who exhausted their budget and then restarted the CLI
//!   could bypass the local refusal — but they would still be
//!   subject to the engine's own limiter, so the net effect is a
//!   5-10 second delay + the engine's 429 path running in lieu of
//!   ours. The complexity of persisting to `~/.zero/state/rate.json`
//!   is not worth that narrow hole.

use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;

/// Wall-clock abstraction so the bucket can be exercised in tests
/// without sleeping. Only `now()` is on the trait surface; every
/// internal math op is pure and does not need further mocking.
pub trait Clock: Send + Sync + 'static {
    /// A monotonically-nondecreasing instant. Callers must rely on
    /// the return values' ordering, not their absolute origin.
    fn now(&self) -> Instant;
}

/// Wall-clock implementation — the one production always uses.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
}

/// Manual clock for tests. Wrap `Arc<ManualClock>`; a test that
/// wants to advance the clock holds a handle next to the
/// `RateBudget` it feeds into.
///
/// Uses a `Mutex<Instant>` rather than atomics because the `Instant`
/// type is not trivially atomic across platforms and a test-only
/// code path is not where cycle-shaving matters.
#[derive(Debug)]
pub struct ManualClock {
    now: Mutex<Instant>,
}

impl ManualClock {
    /// Seed at an arbitrary instant. Callers should treat the
    /// seed as opaque — only deltas matter.
    #[must_use]
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            now: Mutex::new(Instant::now()),
        })
    }

    /// Advance the clock by `d`. Useful for `advance(Duration::
    /// from_secs(5))` to let the bucket refill, etc.
    pub fn advance(&self, d: Duration) {
        let mut t = self.now.lock();
        *t += d;
    }
}

impl Clock for ManualClock {
    fn now(&self) -> Instant {
        *self.now.lock()
    }
}

/// Per-endpoint cost table. The costs themselves live here as
/// `const` so tests, docs, the status bar widget, and the budget
/// itself all read from a single source — otherwise a change to
/// the cost of `/v2/status` in the client and a stale cost in the
/// doctor row is a silent drift waiting to happen.
///
/// Costs follow M2_PLAN §1:
///
/// - **1 point:** the cheap rollups the CLI reads for chrome —
///   `/`, `/health`, `/status`, `/risk`, `/positions`, `/brief`,
///   `/regime`, `/approaching`, `/rejections`, `/hl/status`, `/market/quote`,
///   `/operator/state`, and `POST /operator/events` (append-only, cheap).
/// - **2 points:** endpoints that trigger meaningful engine work —
///   `/evaluate/{coin}` (runs the verdict pipeline against live
///   features) and `/pulse` (journals out a recent event cross-
///   section).
/// - **3 points:** composite endpoints — `/v2/status`, which joins
///   several sub-objects into a single payload.
///
/// The path's query string is stripped before lookup so that
/// `/evaluate/BTC?foo=1` costs the same as `/evaluate/BTC`.
#[must_use]
pub fn cost_of(path: &str) -> u32 {
    // Strip the query string if present; match only on the path
    // prefix so /evaluate/{coin} covers every coin without a
    // per-coin row.
    let path = path.split('?').next().unwrap_or(path);
    if path.starts_with("/evaluate") || path == "/pulse" || path.starts_with("/pulse?") {
        2
    } else if path == "/v2/status" {
        3
    } else {
        1
    }
}

/// Default bucket capacity — the burst size. 60 tokens lets a busy
/// operator run ~20 `/v2/status` renders (cost 3 each) in a tight
/// burst without hitting the floor, which covers the observed
/// peak-typing pattern (rapid `/status` + `/risk` + `/positions`
/// walk at session open).
pub const DEFAULT_CAPACITY: u32 = 60;

/// Default refill rate — 1 token per second. 60 per minute
/// sustained matches the engine's per-operator 429 floor observed
/// in the existing Python surface (see `engine/zero/auth.py`'s
/// `_RATE_LIMIT` constant, ~60 reqs/min). Staying under it means
/// the CLI-side bucket trips before the engine's ever does,
/// guaranteeing the operator sees a typed refusal rather than a
/// blanket 429.
pub const DEFAULT_REFILL_PER_SECOND: f64 = 1.0;

#[derive(Debug)]
struct State {
    /// Current token count. Stored as `f64` so the per-tick
    /// accrual (`refill_per_sec * elapsed_seconds`) does not
    /// truncate to zero between two sub-second calls — a common
    /// mistake in integer-bucket implementations is losing
    /// sub-second accrual. Capped at `capacity` on every refill.
    tokens: f64,
    last_refill: Instant,
}

struct Inner {
    capacity: u32,
    refill_per_second: f64,
    clock: Arc<dyn Clock>,
    state: Mutex<State>,
}

// Hand-rolled `Debug` because `dyn Clock` is intentionally not
// `Debug`-bound (minimal trait surface) and the auto-derive would
// refuse. The fields we can render are the ones a `#[derive]`
// would have surfaced anyway.
impl std::fmt::Debug for Inner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Inner")
            .field("capacity", &self.capacity)
            .field("refill_per_second", &self.refill_per_second)
            .field("state", &self.state)
            .finish_non_exhaustive()
    }
}

/// A cloneable, thread-safe token bucket. Cheap to clone (bumps an
/// `Arc`); the inner state is shared, which is the whole point.
#[derive(Clone)]
pub struct RateBudget {
    inner: Arc<Inner>,
}

impl std::fmt::Debug for RateBudget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let snap = self.snapshot();
        f.debug_struct("RateBudget")
            .field("capacity", &snap.capacity)
            .field("refill_per_second", &snap.refill_per_second)
            .field("tokens", &snap.tokens)
            .finish()
    }
}

/// Read-only view of the bucket state. Returned by
/// [`RateBudget::snapshot`] for the doctor row and the status-bar
/// widget — neither should hold the internal mutex.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BudgetSnapshot {
    pub capacity: u32,
    pub refill_per_second: f64,
    /// Current tokens, floored to integer for display. Callers who
    /// want the raw float do not exist today; if one shows up,
    /// add a separate accessor rather than growing this row.
    pub tokens: u32,
}

impl BudgetSnapshot {
    /// Fraction of capacity still available, in `0.0..=1.0`. The
    /// status-bar widget uses this to pick a color band:
    ///
    /// - ≥ 0.25 → primary
    /// - 0.10..0.25 → caution
    /// - < 0.10 → alert
    /// - 0.0 → `EXH` (rendered by the widget, not this number)
    #[must_use]
    pub fn headroom(&self) -> f64 {
        if self.capacity == 0 {
            0.0
        } else {
            f64::from(self.tokens) / f64::from(self.capacity)
        }
    }
}

/// What `try_consume` returns when the bucket cannot satisfy the
/// requested cost. `retry_after` is how long (rounded up) until
/// enough tokens will have accrued to complete the call.
///
/// Rounding up matters: an operator-visible "retry in 0s" is a
/// lie when the real answer is "retry in 400 ms" — we floor to
/// the next whole second so the countdown in the status bar and
/// the HttpError it becomes never advertise a shorter wait than
/// is actually needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Exhausted {
    pub retry_after: Duration,
}

impl RateBudget {
    /// Build a bucket with the default capacity + refill. Clock
    /// is [`SystemClock`] (the only right answer in production).
    #[must_use]
    pub fn default_system() -> Self {
        Self::with_clock(
            DEFAULT_CAPACITY,
            DEFAULT_REFILL_PER_SECOND,
            Arc::new(SystemClock),
        )
    }

    /// Build a bucket with explicit capacity, refill rate, and
    /// clock. Panics if `capacity == 0` (a zero-capacity bucket
    /// is never usable and always exhausts; callers intending
    /// "infinitely permissive" should not wire the bucket in at
    /// all) or `refill_per_second < 0.0` (a negative refill is
    /// incoherent). A refill of 0.0 is permitted — useful in tests
    /// that want to prove the exhaustion path without accrual
    /// confounding the observation.
    ///
    /// # Panics
    ///
    /// If `capacity == 0` or `refill_per_second.is_sign_negative()`
    /// or `refill_per_second.is_nan()`. All three are programmer
    /// errors caught at construction rather than surfaced as
    /// silent "never allows anything" behavior downstream.
    #[must_use]
    pub fn with_clock(capacity: u32, refill_per_second: f64, clock: Arc<dyn Clock>) -> Self {
        assert!(capacity > 0, "rate-budget capacity must be > 0");
        assert!(
            refill_per_second.is_finite() && !refill_per_second.is_sign_negative(),
            "rate-budget refill must be a finite, non-negative float (got {refill_per_second})"
        );
        let now = clock.now();
        let state = State {
            tokens: f64::from(capacity),
            last_refill: now,
        };
        Self {
            inner: Arc::new(Inner {
                capacity,
                refill_per_second,
                clock,
                state: Mutex::new(state),
            }),
        }
    }

    /// Attempt to consume `cost` tokens. Returns `Ok(())` on
    /// success (the bucket has been debited), or `Err(Exhausted)`
    /// with a floor-rounded `retry_after` when the bucket cannot
    /// satisfy the cost.
    ///
    /// A cost that exceeds `capacity` can never be satisfied —
    /// the caller gets an `Exhausted { retry_after }` shaped as
    /// "forever-ish" (`Duration::MAX`). This is a misconfiguration
    /// signal, not a transient error; the caller should surface it
    /// loudly. In practice the cost table's highest value (3) is
    /// well below the default capacity (60), so this path is a
    /// developer-error canary, not a production concern.
    pub fn try_consume(&self, cost: u32) -> Result<(), Exhausted> {
        let cost_f = f64::from(cost);
        let mut state = self.inner.state.lock();
        self.refill_locked(&mut state);

        if state.tokens >= cost_f {
            state.tokens -= cost_f;
            return Ok(());
        }

        // Cost exceeds what even a full bucket can satisfy —
        // permanent exhaustion (as far as this bucket is
        // concerned). Returning `Duration::MAX` is intentional:
        // downstream HttpError surfaces it as "call the budget
        // broken," not as a countdown the operator might wait
        // through.
        if cost_f > f64::from(self.inner.capacity) {
            return Err(Exhausted {
                retry_after: Duration::MAX,
            });
        }

        // The bucket will have `cost_f - state.tokens` more tokens
        // after `deficit / refill_per_second` seconds. Floor to the
        // next whole second so the caller's countdown is never
        // shorter than the truth; if refill is zero, we stall
        // forever (and signal it via `Duration::MAX`).
        let deficit = cost_f - state.tokens;
        let retry = if self.inner.refill_per_second > 0.0 {
            let secs = (deficit / self.inner.refill_per_second).ceil();
            // `secs` is finite and non-negative here (deficit > 0,
            // refill > 0). `Duration::try_from_secs_f64` clamps to
            // the representable range on the high end and would
            // fail on NaN — which we already ruled out. A plain
            // `unwrap_or(Duration::MAX)` is both honest (saturates
            // to the "forever" sentinel on pathological refills)
            // and keeps clippy happy without an allow-list dance
            // around manual `as u64` casts.
            let candidate = Duration::try_from_secs_f64(secs).unwrap_or(Duration::MAX);
            candidate.max(Duration::from_secs(1))
        } else {
            Duration::MAX
        };
        Err(Exhausted { retry_after: retry })
    }

    /// Refund `cost` tokens — used when an outer rate limiter
    /// (the engine's own 429) fires after we already debited our
    /// local bucket. Without this, every 429 would double-charge
    /// the operator: once against our bucket, once against the
    /// engine's. Capped at `capacity` so a runaway refund bug
    /// cannot inflate the bucket beyond its design size.
    pub fn refund(&self, cost: u32) {
        let mut state = self.inner.state.lock();
        state.tokens = (state.tokens + f64::from(cost)).min(f64::from(self.inner.capacity));
    }

    /// Snapshot for display. Runs the refill pass so the returned
    /// token count reflects elapsed time, not the last `try_consume`
    /// ago — a status bar that reads `rate:40/60` when the real
    /// answer is `rate:55/60` paints a fake scarcity.
    #[must_use]
    pub fn snapshot(&self) -> BudgetSnapshot {
        let mut state = self.inner.state.lock();
        self.refill_locked(&mut state);
        BudgetSnapshot {
            capacity: self.inner.capacity,
            refill_per_second: self.inner.refill_per_second,
            // Saturate: f64 → u32 via `as u32` with the caller
            // guaranteeing the value is within the bucket range.
            // `state.tokens` is in `[0, capacity]` by construction
            // so saturation is belt-and-braces only.
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            tokens: state.tokens.floor().max(0.0).min(f64::from(u32::MAX)) as u32,
        }
    }

    /// Force the bucket full. Only wired through
    /// `zero doctor --fix` (M2_PLAN §1's clear-counter action) —
    /// operator confirmation is required before this runs, because
    /// bypassing the local bucket has no production use case. The
    /// name is loud on purpose.
    pub fn reset_to_full(&self) {
        let mut state = self.inner.state.lock();
        state.tokens = f64::from(self.inner.capacity);
        state.last_refill = self.inner.clock.now();
    }

    /// Advance `state.tokens` by the accrual since `last_refill`
    /// and update `last_refill` to the current clock reading.
    ///
    /// Accrual is `elapsed_seconds * refill_per_second`, capped at
    /// `capacity`. Called under the state lock; the `_locked`
    /// suffix reminds the reader of that invariant.
    fn refill_locked(&self, state: &mut State) {
        let now = self.inner.clock.now();
        let elapsed = now.duration_since(state.last_refill);
        if elapsed.is_zero() {
            return;
        }
        let accrual = elapsed.as_secs_f64() * self.inner.refill_per_second;
        state.tokens = (state.tokens + accrual).min(f64::from(self.inner.capacity));
        state.last_refill = now;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bucket(cap: u32, refill: f64) -> (RateBudget, Arc<ManualClock>) {
        let clock = ManualClock::new();
        let b = RateBudget::with_clock(cap, refill, clock.clone());
        (b, clock)
    }

    #[test]
    fn costs_follow_spec_table() {
        // Cheap rollups.
        assert_eq!(cost_of("/status"), 1);
        assert_eq!(cost_of("/risk"), 1);
        assert_eq!(cost_of("/positions"), 1);
        assert_eq!(cost_of("/brief"), 1);
        assert_eq!(cost_of("/regime"), 1);
        assert_eq!(cost_of("/operator/state"), 1);
        assert_eq!(cost_of("/operator/events"), 1);
        assert_eq!(cost_of("/approaching"), 1);
        assert_eq!(cost_of("/rejections"), 1);
        assert_eq!(cost_of("/hl/status"), 1);
        assert_eq!(cost_of("/hl/status?symbol=BTC"), 1);
        assert_eq!(cost_of("/market/quote?symbol=BTC"), 1);

        // Engine-work endpoints.
        assert_eq!(cost_of("/evaluate/BTC"), 2);
        assert_eq!(cost_of("/evaluate/BTC?side=long"), 2);
        assert_eq!(cost_of("/pulse"), 2);
        assert_eq!(cost_of("/pulse?limit=50"), 2);

        // Composite.
        assert_eq!(cost_of("/v2/status"), 3);
    }

    #[test]
    fn new_bucket_is_full() {
        let (b, _clock) = bucket(10, 1.0);
        assert_eq!(b.snapshot().tokens, 10);
    }

    #[test]
    fn consume_debits_tokens() {
        let (b, _clock) = bucket(10, 0.0);
        assert!(b.try_consume(3).is_ok());
        assert_eq!(b.snapshot().tokens, 7);
        assert!(b.try_consume(7).is_ok());
        assert_eq!(b.snapshot().tokens, 0);
    }

    #[test]
    fn consume_exhaustion_returns_floored_retry_after() {
        // Cap 10, no refill → consume all, next call says "never."
        let (b, _clock) = bucket(10, 0.0);
        assert!(b.try_consume(10).is_ok());
        let err = b.try_consume(1).unwrap_err();
        assert_eq!(err.retry_after, Duration::MAX);
    }

    #[test]
    fn consume_exhaustion_countdown_rounds_up() {
        // Cap 10, 1 token/sec. Consume 10, ask for 3. The bucket
        // will have 3 tokens after 3 seconds. Countdown must be 3 s,
        // not 2 s, not 4 s.
        let (b, _clock) = bucket(10, 1.0);
        b.try_consume(10).unwrap();
        let err = b.try_consume(3).unwrap_err();
        assert_eq!(err.retry_after, Duration::from_secs(3));
    }

    #[test]
    fn consume_exhaustion_fractional_deficit_rounds_up() {
        // Cap 10, 2 tokens/sec. Consume 10, ask for 3. Refill rate
        // says "1.5 seconds" — the operator-visible retry must be
        // 2 s (ceiling), never 1 s.
        let (b, _clock) = bucket(10, 2.0);
        b.try_consume(10).unwrap();
        let err = b.try_consume(3).unwrap_err();
        assert_eq!(err.retry_after, Duration::from_secs(2));
    }

    #[test]
    fn refill_accrues_over_clock_advance() {
        let (b, clock) = bucket(10, 1.0);
        b.try_consume(10).unwrap();
        clock.advance(Duration::from_secs(5));
        // 5 tokens accrued.
        assert_eq!(b.snapshot().tokens, 5);
    }

    #[test]
    fn refill_caps_at_capacity() {
        let (b, clock) = bucket(10, 1.0);
        b.try_consume(2).unwrap();
        // Advance by far more than the capacity deficit — the
        // bucket must not overflow.
        clock.advance(Duration::from_secs(1000));
        assert_eq!(b.snapshot().tokens, 10);
    }

    #[test]
    fn sub_second_accrual_does_not_floor_to_zero() {
        // A naive integer-bucket would lose the 100 ms accrual
        // here and sit at `tokens = 0` forever under rapid polling.
        // The float-accumulator implementation must track it.
        let (b, clock) = bucket(10, 10.0); // 10 tokens/sec
        b.try_consume(10).unwrap();
        clock.advance(Duration::from_millis(100)); // 1 token
        clock.advance(Duration::from_millis(100)); // 2 tokens
        assert_eq!(b.snapshot().tokens, 2);
    }

    #[test]
    fn refund_restores_tokens_without_exceeding_capacity() {
        let (b, _clock) = bucket(10, 0.0);
        b.try_consume(5).unwrap();
        b.refund(5);
        assert_eq!(b.snapshot().tokens, 10);
        // Extra refund is a no-op (bug shield).
        b.refund(5);
        assert_eq!(b.snapshot().tokens, 10);
    }

    #[test]
    fn reset_to_full_refills_the_bucket() {
        let (b, _clock) = bucket(10, 0.0);
        b.try_consume(10).unwrap();
        assert_eq!(b.snapshot().tokens, 0);
        b.reset_to_full();
        assert_eq!(b.snapshot().tokens, 10);
    }

    #[test]
    fn headroom_bands_are_legible() {
        let snap = BudgetSnapshot {
            capacity: 60,
            refill_per_second: 1.0,
            tokens: 60,
        };
        assert!((snap.headroom() - 1.0).abs() < f64::EPSILON);
        let half = BudgetSnapshot { tokens: 30, ..snap };
        assert!((half.headroom() - 0.5).abs() < f64::EPSILON);
        let empty = BudgetSnapshot { tokens: 0, ..snap };
        assert!(empty.headroom().abs() < f64::EPSILON);
    }

    #[test]
    fn cost_above_capacity_returns_permanent_exhaustion() {
        // A 60-token bucket cannot satisfy a 61-cost call. The
        // misconfiguration signal must be a `Duration::MAX` retry
        // so downstream renderers flag it as a config bug, not a
        // countdown the operator might wait out.
        let (b, _clock) = bucket(60, 1.0);
        let err = b.try_consume(61).unwrap_err();
        assert_eq!(err.retry_after, Duration::MAX);
    }

    #[test]
    #[should_panic(expected = "capacity must be > 0")]
    fn zero_capacity_panics_at_construction() {
        let _ = RateBudget::with_clock(0, 1.0, Arc::new(SystemClock));
    }

    #[test]
    #[should_panic(expected = "refill must be a finite, non-negative float")]
    fn negative_refill_panics_at_construction() {
        let _ = RateBudget::with_clock(10, -1.0, Arc::new(SystemClock));
    }
}
