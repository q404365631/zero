//! Responsive layout per spec §4.4.
//!
//! Width bands:
//!   - `>=120` full status bar, live-stream pane visible
//!   - `80..120` compact status bar, live-stream toggleable
//!   - `60..80` minimal status bar, live-stream off
//!   - `<60` degraded warning

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Width {
    Full,
    Compact,
    Minimal,
    Degraded,
}

impl Width {
    #[must_use]
    pub const fn classify(cols: u16) -> Self {
        if cols >= 120 {
            Self::Full
        } else if cols >= 80 {
            Self::Compact
        } else if cols >= 60 {
            Self::Minimal
        } else {
            Self::Degraded
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Width;

    #[test]
    fn bands() {
        assert_eq!(Width::classify(200), Width::Full);
        assert_eq!(Width::classify(120), Width::Full);
        assert_eq!(Width::classify(119), Width::Compact);
        assert_eq!(Width::classify(80), Width::Compact);
        assert_eq!(Width::classify(79), Width::Minimal);
        assert_eq!(Width::classify(60), Width::Minimal);
        assert_eq!(Width::classify(59), Width::Degraded);
    }
}
