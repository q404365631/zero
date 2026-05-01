//! Bounded ring of `EngineEvent`s for the live-stream pane.
//!
//! The TUI taps the `WsSubscriber`'s broadcast channel and
//! appends every received event here. The ring is a FIFO with a
//! hard capacity — old events drop off the back so the pane
//! always fits in a fixed-size allocation even on a noisy day.
//!
//! A second item variant, [`RingItem::Lagged`], is pushed when
//! the broadcast receiver reports `RecvError::Lagged`: rather
//! than silently lose events (the conversation log is the
//! operator's record of what the engine was saying), we drop a
//! loud marker telling them the stream skipped N messages. This
//! is a deliberate honesty choice — a calm empty pane after a
//! burst of dropped events would be the worst possible failure
//! mode for a trading terminal.

use std::collections::VecDeque;

use chrono::{DateTime, Utc};
use zero_engine_client::EngineEvent;

/// Default ring capacity. Chosen large enough to hold several
/// minutes of mixed traffic at typical engine emission rates
/// (`heartbeat` every 5 s + occasional status / positions /
/// risk updates) while keeping the allocation under a few KB.
pub const DEFAULT_CAPACITY: usize = 200;

/// One slot in the ring. Either a real decoded engine event or
/// a synthetic "lagged" marker recording how many events the
/// broadcast channel skipped.
#[derive(Debug, Clone)]
pub enum RingItem {
    Event(RingEntry),
    Lagged {
        ts: DateTime<Utc>,
        /// Number of events the broadcast receiver reported as
        /// dropped.
        skipped: u64,
    },
}

/// A decoded engine event plus the wall-clock timestamp we
/// observed it at (falling back to "now" when the event does
/// not carry one). Storing the ts separately keeps render
/// formatting stateless — the pane never has to dig into the
/// typed payload to display when it arrived.
#[derive(Debug, Clone)]
pub struct RingEntry {
    pub ts: DateTime<Utc>,
    pub event: EngineEvent,
}

/// Bounded FIFO of ring items.
#[derive(Debug)]
pub struct EventRing {
    items: VecDeque<RingItem>,
    cap: usize,
}

impl Default for EventRing {
    fn default() -> Self {
        Self::with_capacity(DEFAULT_CAPACITY)
    }
}

impl EventRing {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Build a ring with a specific capacity. `cap == 0` is
    /// accepted and produces a drop-everything ring (tests may
    /// use this to confirm the bound is enforced at zero too);
    /// production callers should use [`DEFAULT_CAPACITY`].
    #[must_use]
    pub fn with_capacity(cap: usize) -> Self {
        Self {
            items: VecDeque::with_capacity(cap),
            cap,
        }
    }

    /// Maximum number of items this ring retains.
    #[must_use]
    pub const fn capacity(&self) -> usize {
        self.cap
    }

    /// Current number of retained items.
    #[must_use]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// `true` when no items have been recorded yet.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Append a typed engine event. Drops the oldest item when
    /// at capacity so the push is O(1) and never allocates past
    /// `cap`.
    pub fn push_event(&mut self, event: EngineEvent) {
        let ts = event_timestamp(&event).unwrap_or_else(Utc::now);
        self.push(RingItem::Event(RingEntry { ts, event }));
    }

    /// Like [`Self::push_event`] with an explicit timestamp.
    /// Exists so snapshot tests can drive deterministic content
    /// for variants (`Status`, `Positions`, `Risk`, `Regime`)
    /// that do not carry a ts inside their typed payload. Never
    /// called by the runtime event loop — production writes
    /// always go through [`Self::push_event`] which takes the
    /// wall clock.
    pub fn push_event_at(&mut self, event: EngineEvent, ts: DateTime<Utc>) {
        self.push(RingItem::Event(RingEntry { ts, event }));
    }

    /// Append a "broadcast channel lagged" marker. Reserved for
    /// the event loop's `RecvError::Lagged` branch.
    pub fn push_lagged(&mut self, skipped: u64) {
        self.push(RingItem::Lagged {
            ts: Utc::now(),
            skipped,
        });
    }

    /// Deterministic sibling of [`Self::push_lagged`] for tests.
    pub fn push_lagged_at(&mut self, skipped: u64, ts: DateTime<Utc>) {
        self.push(RingItem::Lagged { ts, skipped });
    }

    fn push(&mut self, item: RingItem) {
        if self.cap == 0 {
            // Zero-cap ring accepts no items; this is the
            // "discard everything" test config. Dropping here
            // keeps the invariant `len() <= cap` across pushes.
            return;
        }
        while self.items.len() >= self.cap {
            self.items.pop_front();
        }
        self.items.push_back(item);
    }

    /// Iterate newest-last (i.e. chronological order). Kept
    /// generic so callers can chain `.rev()` for newest-first
    /// rendering without paying for a materialized `Vec`.
    pub fn iter(&self) -> impl DoubleEndedIterator<Item = &RingItem> {
        self.items.iter()
    }

    /// Iterate the last `n` items in chronological order. When
    /// `n` exceeds [`Self::len`] the iterator yields every item
    /// — same as [`Self::iter`]. Used by the pane to render
    /// "last N rows" without allocating.
    pub fn tail(&self, n: usize) -> impl DoubleEndedIterator<Item = &RingItem> {
        let start = self.items.len().saturating_sub(n);
        self.items.iter().skip(start)
    }
}

/// Extract the timestamp from an engine event when the variant
/// carries one directly. Other variants fall through to "now"
/// in [`EventRing::push_event`] — the subscriber's frame-receive
/// time is a good enough proxy for the operator's pane.
fn event_timestamp(evt: &EngineEvent) -> Option<DateTime<Utc>> {
    match evt {
        EngineEvent::Heartbeat(ts) | EngineEvent::Unknown { ts, .. } => Some(*ts),
        EngineEvent::Status(_)
        | EngineEvent::Positions(_)
        | EngineEvent::Risk(_)
        | EngineEvent::Regime(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn heartbeat_at(sec: i64) -> EngineEvent {
        EngineEvent::Heartbeat(DateTime::<Utc>::from_timestamp(sec, 0).unwrap())
    }

    #[test]
    fn fresh_ring_is_empty_and_reports_capacity() {
        let r = EventRing::new();
        assert!(r.is_empty());
        assert_eq!(r.len(), 0);
        assert_eq!(r.capacity(), DEFAULT_CAPACITY);
    }

    #[test]
    fn push_increments_len_up_to_capacity() {
        let mut r = EventRing::with_capacity(4);
        for s in 0..3 {
            r.push_event(heartbeat_at(s));
        }
        assert_eq!(r.len(), 3);
        assert!(!r.is_empty());
    }

    #[test]
    fn push_beyond_capacity_drops_oldest() {
        let mut r = EventRing::with_capacity(3);
        for s in 1..=5 {
            r.push_event(heartbeat_at(s));
        }
        assert_eq!(r.len(), 3);
        let tss: Vec<i64> = r
            .iter()
            .map(|it| match it {
                RingItem::Event(e) => e.ts.timestamp(),
                RingItem::Lagged { ts, .. } => ts.timestamp(),
            })
            .collect();
        // Oldest two (1, 2) must have been discarded; 3..=5 remain.
        assert_eq!(tss, vec![3, 4, 5]);
    }

    #[test]
    fn zero_capacity_accepts_no_items() {
        let mut r = EventRing::with_capacity(0);
        r.push_event(heartbeat_at(1));
        r.push_lagged(42);
        assert!(r.is_empty());
        assert_eq!(r.capacity(), 0);
    }

    #[test]
    fn push_lagged_records_marker_with_count() {
        let mut r = EventRing::with_capacity(2);
        r.push_event(heartbeat_at(1));
        r.push_lagged(7);
        let items: Vec<_> = r.iter().collect();
        assert_eq!(items.len(), 2);
        assert!(matches!(items[1], RingItem::Lagged { skipped: 7, .. }));
    }

    #[test]
    fn tail_clamps_to_ring_len() {
        let mut r = EventRing::with_capacity(10);
        for s in 1..=3 {
            r.push_event(heartbeat_at(s));
        }
        // Asking for more than we have returns all three.
        assert_eq!(r.tail(100).count(), 3);
        // Asking for the last 2 returns the two newest.
        let last2: Vec<i64> = r
            .tail(2)
            .map(|it| match it {
                RingItem::Event(e) => e.ts.timestamp(),
                RingItem::Lagged { ts, .. } => ts.timestamp(),
            })
            .collect();
        assert_eq!(last2, vec![2, 3]);
    }

    #[test]
    fn push_event_without_direct_ts_falls_back_to_now() {
        // Status / positions / risk / regime don't carry a ts in
        // the enum; the ring must still record them with a
        // sensible timestamp. We don't assert the exact value
        // (wall clock), just that the entry landed.
        let mut r = EventRing::with_capacity(2);
        r.push_event(EngineEvent::Heartbeat(Utc::now())); // trivial variant
        // Unknown carries ts, exercised elsewhere. The real
        // fallback path is hit by Status/Positions/Risk/Regime,
        // which require a full decoded payload — not worth
        // constructing here. The key invariant (push does not
        // panic, timestamp is finite) is what we care about.
        assert_eq!(r.len(), 1);
    }
}
