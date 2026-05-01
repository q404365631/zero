//! The four modes (ADR-015). `Calibration`, `Scars`, `Risk`, and
//! `Network` are overlays dispatched by `zero-commands`, not modes.

use std::fmt;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Mode {
    #[default]
    Conversation,
    Positions,
    Decisions,
    Heat,
}

impl Mode {
    /// Short label for the status bar: `CONV`, `POS`, `DEC`, `HEAT`.
    #[must_use]
    pub const fn short(self) -> &'static str {
        match self {
            Self::Conversation => "CONV",
            Self::Positions => "POS",
            Self::Decisions => "DEC",
            Self::Heat => "HEAT",
        }
    }

    /// Full label used in overlays and docs.
    #[must_use]
    pub const fn full(self) -> &'static str {
        match self {
            Self::Conversation => "conversation",
            Self::Positions => "positions",
            Self::Decisions => "decisions",
            Self::Heat => "heat",
        }
    }

    /// Decode a `Ctrl+N` digit (1..=4). `Ctrl+0` returns
    /// [`Mode::Conversation`]. Returns `None` for Ctrl+5..9 —
    /// those bindings are deliberately unbound (ADR-015).
    #[must_use]
    pub const fn from_digit(d: u8) -> Option<Self> {
        match d {
            0 | 1 => Some(Self::Conversation),
            2 => Some(Self::Positions),
            3 => Some(Self::Decisions),
            4 => Some(Self::Heat),
            _ => None,
        }
    }
}

impl fmt::Display for Mode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.full())
    }
}

#[cfg(test)]
mod tests {
    use super::Mode;

    #[test]
    fn digit_mapping() {
        assert_eq!(Mode::from_digit(0), Some(Mode::Conversation));
        assert_eq!(Mode::from_digit(1), Some(Mode::Conversation));
        assert_eq!(Mode::from_digit(2), Some(Mode::Positions));
        assert_eq!(Mode::from_digit(3), Some(Mode::Decisions));
        assert_eq!(Mode::from_digit(4), Some(Mode::Heat));
        for d in 5..=9u8 {
            assert_eq!(Mode::from_digit(d), None, "Ctrl+{d} must be unbound");
        }
    }
}
