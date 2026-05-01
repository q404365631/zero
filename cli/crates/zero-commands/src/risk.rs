//! Risk direction + friction gate invariant (ADR-014).
//!
//! Every operator-initiated command carries a [`RiskDirection`].
//! Risk-reducing actions (`/kill`, `/flatten-all`, `/close`,
//! `/pause-entries`, `/break`) are always instant and friction-exempt.
//! Risk-increasing actions (opening positions, composition changes)
//! pass through [`FrictionGate`], which is parameterized so that
//! only `Increases` can ever be wrapped. Attempting to apply the
//! gate to a `Reduces` or `Neutral` command is a compile error.
//!
//! See also `zero-operator-state::friction::FrictionGate`, which
//! uses the same sealed-trait pattern on the state-vector side.

use serde::{Deserialize, Serialize};

/// The direction a command moves risk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskDirection {
    /// The command can open, enlarge, or resume risk.
    Increases,
    /// The command closes, shrinks, or pauses risk.
    Reduces,
    /// The command changes nothing that affects exposure (reads,
    /// mode switches, log clears).
    Neutral,
}

/// Sealed marker trait — only implemented by [`Increases`] below.
/// External crates cannot implement it, which keeps the invariant
/// "only risk-increasing commands are friction-gated" enforceable
/// at compile time.
pub trait Gateable: sealed::Sealed + Copy + 'static {
    /// Runtime echo of the compile-time direction, for logging.
    const DIRECTION: RiskDirection;
}

/// Phantom marker for compile-time direction checking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Increases;

impl sealed::Sealed for Increases {}
impl Gateable for Increases {
    const DIRECTION: RiskDirection = RiskDirection::Increases;
}

mod sealed {
    pub trait Sealed {}
}

/// Compile-time-checked friction wrapper. Construct via
/// [`FrictionGate::new`], which accepts only [`Increases`]-typed
/// commands; the function signature prevents callers from
/// accidentally friction-wrapping a risk-reducing action.
///
/// # Risk-asymmetry invariant (compile-time)
///
/// The point of the type parameter is to make this line
/// *unable to compile*:
///
/// ```compile_fail
/// use zero_commands::risk::FrictionGate;
///
/// // A local `Reduces` phantom. Not `Gateable` — there is no
/// // way for an external crate to make it `Gateable`, because
/// // `Gateable` is sealed to this crate (see `sealed` module).
/// #[derive(Clone, Copy)]
/// struct Reduces;
///
/// // This must be a compile error, not a runtime one. If it
/// // compiles, the type-level guarantee is gone and a
/// // risk-reducing command could be wrapped in friction.
/// let _: FrictionGate<Reduces> = FrictionGate::new();
/// ```
///
/// The control is also positive: a `FrictionGate<Increases>`
/// is constructible and reports its direction honestly.
///
/// ```
/// use zero_commands::risk::{FrictionGate, Increases, RiskDirection};
/// let g = FrictionGate::<Increases>::new();
/// assert_eq!(g.direction(), RiskDirection::Increases);
/// ```
#[derive(Debug, Clone, Copy)]
pub struct FrictionGate<D: Gateable> {
    _direction: std::marker::PhantomData<D>,
}

impl Default for FrictionGate<Increases> {
    fn default() -> Self {
        Self::new()
    }
}

impl FrictionGate<Increases> {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            _direction: std::marker::PhantomData,
        }
    }

    /// The direction this gate operates on. Useful for logging
    /// when a friction pause is shown to the operator.
    #[must_use]
    pub const fn direction(&self) -> RiskDirection {
        Increases::DIRECTION
    }
}

#[cfg(test)]
mod tests {
    use super::{FrictionGate, Increases, RiskDirection};

    #[test]
    fn gate_reports_direction() {
        let g = FrictionGate::<Increases>::new();
        assert_eq!(g.direction(), RiskDirection::Increases);
    }

    // The compile-fail + positive-construction contracts live
    // as doctests on `FrictionGate` itself so they run under
    // `cargo test --doc` (a `cfg(test)` private doctest would
    // never be collected by the doc-test harness).
}
