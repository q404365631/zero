//! Command-line parser.
//!
//! Input is a single line typed at the prompt. We lex it into a
//! head word (the command name, optionally prefixed with `/`) and
//! a tail of whitespace-separated tokens. Quoting is deliberately
//! not supported — operator commands are terse by design, and
//! anything that needs a sentence belongs in the conversation
//! stream, not in a slash-command.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedLine {
    pub head: String,
    pub args: Vec<String>,
}

impl ParsedLine {
    /// Lowercased, prefix-stripped head. `/BRIEF` and `brief` both
    /// parse to `"brief"`.
    #[must_use]
    pub fn canonical_head(&self) -> String {
        self.head
            .strip_prefix('/')
            .unwrap_or(&self.head)
            .to_ascii_lowercase()
    }

    /// `true` if the input is empty or whitespace-only.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.head.is_empty()
    }
}

/// Tokenize an operator input line.
#[must_use]
pub fn parse_line(line: &str) -> ParsedLine {
    let mut tokens = line.split_whitespace();
    let head = tokens.next().unwrap_or_default().to_string();
    let args = tokens.map(str::to_string).collect();
    ParsedLine { head, args }
}

#[cfg(test)]
mod tests {
    use super::parse_line;

    #[test]
    fn empty_input() {
        let p = parse_line("   ");
        assert!(p.is_empty());
        assert_eq!(p.canonical_head(), "");
    }

    #[test]
    fn slash_prefix_is_optional() {
        assert_eq!(parse_line("/help").canonical_head(), "help");
        assert_eq!(parse_line("help").canonical_head(), "help");
    }

    #[test]
    fn case_insensitive_head() {
        assert_eq!(parse_line("/BRIEF").canonical_head(), "brief");
    }

    #[test]
    fn splits_args() {
        let p = parse_line("/break 15 minutes");
        assert_eq!(p.canonical_head(), "break");
        assert_eq!(p.args, ["15", "minutes"]);
    }
}
