//! Operator-state honesty.
//!
//! This crate answers one question, in one pure data structure, with
//! no I/O:
//!
//! > *"Given the operator's event history up to now, what behavioral
//! > state are they in and what friction level is appropriate?"*
//!
//! The crate does not talk to the engine. It does not read the
//! filesystem. It does not render anything. It takes events in, it
//! produces a snapshot out. Persistence lives in `zero-session`.
//! Rendering lives in `zero-tui`. Friction enforcement lives in
//! `zero-commands`. The separation is deliberate — this crate is the
//! only place where behavioral classification lives, and it must be
//! testable in isolation with nothing but `Vec<Event>` and
//! `DateTime<Utc>`.
//!
//! See **`ZERO_OS_CLI_SPEC_ADDENDUM_A.md`** §§ 1-3, 10. ADR-013 locks
//! this crate's position in the system; ADR-014 locks the
//! [`RiskDirection`] invariant this crate exports.

#![allow(clippy::module_name_repetitions)]

pub mod classifier;
pub mod events;
pub mod friction;
pub mod label;
pub mod snapshot;
pub mod vector;

pub use classifier::Classifier;
pub use events::{Event, EventKind, Outcome, Source};
pub use friction::{FrictionGate, FrictionLevel, RiskContext, RiskDirection};
pub use label::Label;
pub use snapshot::Snapshot;
pub use vector::StateVector;
