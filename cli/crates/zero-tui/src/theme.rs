//! Theme — phosphor / mono / high-contrast.
//!
//! Color use is strict: phosphor for positive / engine output, amber
//! for caution / pending / shadow, red for alert / blocked / loss,
//! muted olive for dampened / pinned / disabled, cool grey for
//! metadata. See spec §4.2.

use ratatui::style::Color;
use zero_operator_state::label::ColorHint;

#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub primary: Color,
    pub caution: Color,
    pub alert: Color,
    pub muted: Color,
    pub metadata: Color,
}

impl Theme {
    #[must_use]
    pub const fn phosphor() -> Self {
        Self {
            primary: Color::Indexed(148),
            caution: Color::Indexed(214),
            alert: Color::Indexed(196),
            muted: Color::Indexed(100),
            metadata: Color::Indexed(244),
        }
    }

    /// Resolve a renderer-agnostic [`ColorHint`] to a concrete
    /// theme color. `Phosphor`→primary, `Amber`→caution, `Red`
    /// →alert, `MutedOlive`→muted. Keeps the operator-state crate
    /// free of ratatui while the widget stays theme-aware.
    #[must_use]
    pub const fn resolve_hint(&self, hint: ColorHint) -> Color {
        match hint {
            ColorHint::Phosphor => self.primary,
            ColorHint::Amber => self.caution,
            ColorHint::Red => self.alert,
            ColorHint::MutedOlive => self.muted,
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::phosphor()
    }
}
