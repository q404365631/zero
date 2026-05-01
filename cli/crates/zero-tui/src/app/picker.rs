//! Slash-command picker — live-filtered list of commands the
//! operator can tab-complete into the prompt.
//!
//! # Activation
//!
//! The picker is **ambient**: every render, `app::state` checks
//! whether the first row of the prompt buffer starts with `/`,
//! and if so, builds a fresh [`SlashPicker`] from the filter
//! string (the characters after the `/`). There is no "open /
//! close" flag — typing the leading slash opens it, deleting it
//! closes it. This matches the "no mode" invariant used by the
//! overlay system and avoids a stale picker hanging around when
//! the operator clears the prompt.
//!
//! The picker yields priority to the friction-pause overlay:
//! when a gate is active, `app::state` suppresses the picker
//! regardless of prompt contents.
//!
//! # Matching
//!
//! The match function is a deliberately simple subsequence
//! scorer (fzf-lite). Each candidate name is walked character
//! by character; a match requires the filter's chars to appear
//! in order. Score is built from:
//!
//! * `-distance_to_prefix` — matches that start at character 0
//!   rank higher than mid-name matches (`/h` prefers `/help`
//!   over `/flat`ten-all with `h` mid-word).
//! * `-cluster_penalty` — contiguous runs are cheaper than
//!   scattered hits (so `/sta` prefers `/status` over `/state`
//!   only because the whole query is contiguous in both; the
//!   tiebreaker falls back to catalog order).
//!
//! No external crate. fzf-rs and nucleo-matcher are both good
//! libraries, but both are 5-figure-LOC deps and we need exactly
//! 14 entries to match — a subsequence scorer is sufficient and
//! zero-dep friendly.

use zero_commands::{COMMAND_CATALOG, CommandInfo};

/// Max visible rows in the picker popup. Six fits the common
/// case (all current commands after a two-char filter) without
/// stealing the conversation pane.
pub const PICKER_MAX_VISIBLE: usize = 6;

#[derive(Debug)]
pub struct SlashPicker {
    /// Filtered + scored entries, highest-score first.
    matches: Vec<SlashMatch>,
    /// Zero-based index into [`SlashPicker::matches`] of the
    /// currently highlighted row. `0` when there are no matches.
    selected: usize,
}

/// One picker row — the catalog entry plus bookkeeping used by
/// the widget to bold the matched chars.
#[derive(Debug, Clone)]
pub struct SlashMatch {
    pub info: CommandInfo,
    /// Char indices (within `info.name`) that matched the filter.
    /// Empty when the filter is empty (everything matches).
    pub matched_chars: Vec<usize>,
}

impl SlashPicker {
    /// Build a picker for a full prompt line (first row only).
    /// Returns `None` when the line does not start with `/`.
    ///
    /// The filter is the text after the leading slash, truncated
    /// at the first whitespace: `/pos BTC` filters on `pos`.
    #[must_use]
    pub fn from_prompt_line(first_line: &str) -> Option<Self> {
        let rest = first_line.strip_prefix('/')?;
        let filter: String = rest.chars().take_while(|c| !c.is_whitespace()).collect();
        Some(Self::filter_catalog(&filter))
    }

    fn filter_catalog(filter: &str) -> Self {
        let needle = filter.to_ascii_lowercase();
        let mut scored: Vec<(i64, SlashMatch)> = Vec::new();
        for info in COMMAND_CATALOG {
            // Strip the leading `/` on the candidate so the filter
            // `"h"` matches `help` at position 0, not position 1.
            let candidate = info.name.strip_prefix('/').unwrap_or(info.name);
            if let Some((score, matched_chars)) = fuzzy_score(&needle, candidate) {
                // Shift matched indices by +1 so callers can use
                // them against `info.name` (which still has `/`).
                let shifted = matched_chars.iter().map(|i| i + 1).collect();
                scored.push((
                    score,
                    SlashMatch {
                        info: *info,
                        matched_chars: shifted,
                    },
                ));
            }
        }
        // Descending score, then catalog order preserved for ties.
        scored.sort_by(|a, b| b.0.cmp(&a.0));
        let matches: Vec<SlashMatch> = scored.into_iter().map(|(_, m)| m).collect();
        Self {
            matches,
            selected: 0,
        }
    }

    /// Picker is *active* when there is at least one match to
    /// show. An inactive picker renders nothing.
    #[must_use]
    pub fn is_active(&self) -> bool {
        !self.matches.is_empty()
    }

    #[must_use]
    pub fn matches(&self) -> &[SlashMatch] {
        &self.matches
    }

    #[must_use]
    pub const fn selected_index(&self) -> usize {
        self.selected
    }

    /// Currently highlighted entry, or `None` when inactive.
    #[must_use]
    pub fn selected(&self) -> Option<&SlashMatch> {
        self.matches.get(self.selected)
    }

    /// Move selection down (wraps to top at the bottom).
    pub fn select_next(&mut self) {
        if self.matches.is_empty() {
            return;
        }
        self.selected = (self.selected + 1) % self.matches.len();
    }

    /// Move selection up (wraps to bottom at the top).
    pub fn select_prev(&mut self) {
        if self.matches.is_empty() {
            return;
        }
        self.selected = if self.selected == 0 {
            self.matches.len() - 1
        } else {
            self.selected - 1
        };
    }

    /// After Tab-complete, the caller replaces the prompt with
    /// this literal. A trailing space is appended so the operator
    /// can immediately type arguments (e.g. `/regime BTC`).
    #[must_use]
    pub fn completion_text(&self) -> Option<String> {
        self.selected().map(|m| format!("{} ", m.info.name))
    }
}

/// Subsequence scorer. Returns `None` if `needle` cannot be
/// matched as a subsequence of `haystack`; otherwise returns
/// `(score, matched_char_positions)`. Higher score is better.
///
/// Scores are computed entirely in `i64` so picker sorting stays
/// deterministic on 32-bit and 64-bit targets without casting
/// through `usize → i32` (which clippy rightly flags as
/// wrap-prone).
fn fuzzy_score(needle: &str, haystack: &str) -> Option<(i64, Vec<usize>)> {
    if needle.is_empty() {
        return Some((0, Vec::new()));
    }
    let hay: Vec<char> = haystack.chars().map(|c| c.to_ascii_lowercase()).collect();
    let pat: Vec<char> = needle.chars().collect();
    let mut matched: Vec<usize> = Vec::with_capacity(pat.len());
    let mut hi = 0usize;
    for &p in &pat {
        let mut found = None;
        while hi < hay.len() {
            if hay[hi] == p {
                found = Some(hi);
                hi += 1;
                break;
            }
            hi += 1;
        }
        matched.push(found?);
    }

    // Score: prefix match is best (reward `first_index == 0`),
    // then reward contiguity (each adjacent pair saves a gap
    // penalty). All arithmetic is i64 to avoid platform-dependent
    // casts.
    let first = i64::try_from(matched[0]).unwrap_or(i64::MAX);
    let contiguous: i64 = matched
        .windows(2)
        .filter(|pair| pair[1] == pair[0] + 1)
        .count()
        .try_into()
        .unwrap_or(i64::MAX);
    // Exact-prefix bonus. `/h` on `help` beats `/h` on `flatten`.
    let prefix_bonus: i64 = if first == 0 { 50 } else { 0 };
    let score = prefix_bonus + contiguous * 10 - first;
    Some((score, matched))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_slash_means_no_picker() {
        assert!(SlashPicker::from_prompt_line("hello").is_none());
        assert!(SlashPicker::from_prompt_line("").is_none());
    }

    #[test]
    fn empty_filter_lists_every_entry_in_catalog_order() {
        let p = SlashPicker::from_prompt_line("/").expect("picker");
        assert_eq!(p.matches().len(), COMMAND_CATALOG.len());
        // With empty filter, scoring is flat (0); sort is stable,
        // so catalog order is preserved.
        for (i, m) in p.matches().iter().enumerate() {
            assert_eq!(m.info.name, COMMAND_CATALOG[i].name);
        }
    }

    #[test]
    fn filter_narrows_and_orders_by_prefix_match() {
        let p = SlashPicker::from_prompt_line("/st").expect("picker");
        // Expected matches: /status, /state. Both prefix-match,
        // but /state and /status both begin with "st" — tie
        // broken by catalog order (status listed before state).
        let names: Vec<&str> = p.matches().iter().map(|m| m.info.name).collect();
        assert!(names.contains(&"/status"), "want /status in {names:?}");
        assert!(names.contains(&"/state"), "want /state in {names:?}");
        assert_eq!(names[0], "/status", "catalog ordering should be preserved");
    }

    #[test]
    fn fuzzy_subsequence_match() {
        // "pe" matches /pause-entries (subsequence p..e) and
        // /pos is excluded because there is no `e`.
        let p = SlashPicker::from_prompt_line("/pe").expect("picker");
        let names: Vec<&str> = p.matches().iter().map(|m| m.info.name).collect();
        assert!(names.contains(&"/pause-entries"));
        assert!(!names.contains(&"/pos"));
    }

    #[test]
    fn selection_wraps_in_both_directions() {
        let mut p = SlashPicker::from_prompt_line("/st").expect("picker");
        let len = p.matches().len();
        assert!(len >= 2);
        let orig = p.selected_index();
        for _ in 0..len {
            p.select_next();
        }
        assert_eq!(p.selected_index(), orig, "next wraps at len");
        p.select_prev();
        assert_eq!(p.selected_index(), (orig + len - 1) % len);
    }

    #[test]
    fn completion_text_appends_trailing_space() {
        let p = SlashPicker::from_prompt_line("/he").expect("picker");
        let comp = p.completion_text().expect("selected");
        assert!(comp.ends_with(' '));
        assert!(comp.trim_end().starts_with('/'));
    }

    #[test]
    fn filter_stops_at_first_space() {
        // The operator typed `/regime BTC` — filter is `regime`.
        let p = SlashPicker::from_prompt_line("/regime BTC").expect("picker");
        let names: Vec<&str> = p.matches().iter().map(|m| m.info.name).collect();
        assert_eq!(names[0], "/regime");
    }

    #[test]
    fn matched_char_indices_point_into_name() {
        let p = SlashPicker::from_prompt_line("/he").expect("picker");
        let first = &p.matches()[0];
        // Indices are into `info.name`, which includes the `/`.
        for &i in &first.matched_chars {
            assert!(
                i > 0 && i < first.info.name.chars().count(),
                "index {i} out of bounds for {}",
                first.info.name
            );
        }
    }
}
