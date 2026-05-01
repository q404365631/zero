//! State labels, straight from Addendum A §2.3.
//!
//! Labels are computed from a [`crate::StateVector`] via
//! [`crate::Classifier`]. They are rendered by `zero-tui` in the
//! status bar and the `/state` overlay.

use serde::{Deserialize, Serialize};

/// Operator behavioral state.
///
/// Labels are **descriptive, not judgmental** (Addendum A §2.4).
/// The variants match §2.3 exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Label {
    /// <5 decisions in last hour, session <2h.
    Fresh,
    /// Normal velocity, deviation rate <20%, no concerning patterns.
    Steady,
    /// Velocity 1.5x baseline OR deviation 20-40% OR session 4h+.
    Elevated,
    /// Velocity 2x baseline AND (deviation >40% OR loss-reaction <2min).
    Tilt,
    /// Session >6h continuous OR sleep proxy >18h.
    Fatigued,
    /// Post-tilt cooldown active.
    Recovery,
}

impl Label {
    /// Screen-rendered short form.
    ///
    /// Used in the status bar where space is scarce. The `/state`
    /// overlay uses the full Display impl.
    #[must_use]
    pub const fn short(self) -> &'static str {
        match self {
            Self::Fresh => "FRESH",
            Self::Steady => "STEADY",
            Self::Elevated => "ELEVATED",
            Self::Tilt => "TILT",
            Self::Fatigued => "FATIGUED",
            Self::Recovery => "RECOVERY",
        }
    }

    /// Theme hint for the renderer. Maps 1:1 to the colors in
    /// Addendum A §2.3.
    #[must_use]
    pub const fn color_hint(self) -> ColorHint {
        match self {
            Self::Fresh | Self::Steady => ColorHint::Phosphor,
            Self::Elevated | Self::Fatigued => ColorHint::Amber,
            Self::Tilt => ColorHint::Red,
            Self::Recovery => ColorHint::MutedOlive,
        }
    }
}

impl std::fmt::Display for Label {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.short())
    }
}

/// Abstract color identity the `zero-tui` theme resolves to a
/// concrete `ratatui::style::Color`. This crate stays renderer-
/// agnostic so it can be tested without pulling in ratatui.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorHint {
    Phosphor,
    Amber,
    Red,
    MutedOlive,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_labels_are_uppercase_single_word() {
        for l in [
            Label::Fresh,
            Label::Steady,
            Label::Elevated,
            Label::Tilt,
            Label::Fatigued,
            Label::Recovery,
        ] {
            assert!(l.short().chars().all(|c| c.is_ascii_uppercase()));
            assert!(!l.short().contains(' '));
        }
    }

    #[test]
    fn color_hints_match_spec() {
        assert_eq!(Label::Fresh.color_hint(), ColorHint::Phosphor);
        assert_eq!(Label::Steady.color_hint(), ColorHint::Phosphor);
        assert_eq!(Label::Elevated.color_hint(), ColorHint::Amber);
        assert_eq!(Label::Fatigued.color_hint(), ColorHint::Amber);
        assert_eq!(Label::Tilt.color_hint(), ColorHint::Red);
        assert_eq!(Label::Recovery.color_hint(), ColorHint::MutedOlive);
    }
}
