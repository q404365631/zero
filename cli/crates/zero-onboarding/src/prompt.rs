//! Prompt abstraction — all operator I/O goes through the
//! [`Prompt`] trait so the wizard is unit-testable.
//!
//! Production uses [`StdioPrompt`] which reads from stdin and
//! writes to stderr (stdout is reserved for structured non-
//! interactive output). Tests use a `MockPrompt` that scripts
//! answers in advance.

use std::io::{self, BufRead, Write};

use crate::Error;

/// Operator-facing prompter. Methods are synchronous on purpose —
/// the wizard is a line-based wizard, not a TUI, and sync stdio is
/// the simplest correct shape.
pub trait Prompt {
    /// Print a prose line with no trailing prompt. Used for
    /// welcome copy, step headers, and confirmations.
    ///
    /// # Errors
    /// Propagates the underlying I/O error.
    fn say(&mut self, line: &str) -> Result<(), Error>;

    /// Ask a free-text question. `default` is shown in brackets
    /// and returned if the operator hits Enter.
    ///
    /// # Errors
    /// Propagates the underlying I/O error.
    fn ask(&mut self, question: &str, default: Option<&str>) -> Result<String, Error>;

    /// Ask for a secret. Real implementations mask echo; the
    /// `MockPrompt` just returns the scripted answer.
    ///
    /// # Errors
    /// Propagates the underlying I/O error.
    fn ask_secret(&mut self, question: &str) -> Result<String, Error>;

    /// Ask a yes/no question. `default` is the outcome on bare
    /// Enter.
    ///
    /// # Errors
    /// Propagates the underlying I/O error.
    fn confirm(&mut self, question: &str, default: bool) -> Result<bool, Error>;
}

/// Standard-I/O prompter for interactive terminal use.
pub struct StdioPrompt {
    input: Box<dyn BufRead + Send>,
    output: Box<dyn Write + Send>,
}

impl std::fmt::Debug for StdioPrompt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StdioPrompt").finish_non_exhaustive()
    }
}

impl StdioPrompt {
    /// New prompter reading from stdin, writing to stderr.
    #[must_use]
    pub fn stdio() -> Self {
        let stdin = io::stdin();
        let reader = io::BufReader::new(stdin);
        Self {
            input: Box::new(reader),
            output: Box::new(io::stderr()),
        }
    }

    /// New prompter with explicit streams (used in tests).
    pub fn with_streams(
        input: impl BufRead + Send + 'static,
        output: impl Write + Send + 'static,
    ) -> Self {
        Self {
            input: Box::new(input),
            output: Box::new(output),
        }
    }

    fn read_line(&mut self) -> Result<String, Error> {
        let mut buf = String::new();
        let n = self.input.read_line(&mut buf)?;
        if n == 0 {
            return Err(Error::Io(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "stdin closed before answer",
            )));
        }
        Ok(buf.trim_end_matches(['\n', '\r']).to_string())
    }
}

impl Prompt for StdioPrompt {
    fn say(&mut self, line: &str) -> Result<(), Error> {
        writeln!(self.output, "{line}")?;
        self.output.flush()?;
        Ok(())
    }

    fn ask(&mut self, question: &str, default: Option<&str>) -> Result<String, Error> {
        match default {
            Some(d) => write!(self.output, "{question} [{d}]: ")?,
            None => write!(self.output, "{question}: ")?,
        }
        self.output.flush()?;
        let answer = self.read_line()?;
        if answer.is_empty()
            && let Some(d) = default
        {
            return Ok(d.to_string());
        }
        Ok(answer)
    }

    fn ask_secret(&mut self, question: &str) -> Result<String, Error> {
        // Terminal-echo masking is the operator's responsibility —
        // we cannot reliably mask without a platform-specific raw-
        // mode guard, and getting it wrong is worse than not
        // trying. We note the risk in the prompt copy instead.
        write!(self.output, "{question} (input will echo): ")?;
        self.output.flush()?;
        self.read_line()
    }

    fn confirm(&mut self, question: &str, default: bool) -> Result<bool, Error> {
        let hint = if default { "[Y/n]" } else { "[y/N]" };
        write!(self.output, "{question} {hint} ")?;
        self.output.flush()?;
        let answer = self.read_line()?.trim().to_ascii_lowercase();
        if answer.is_empty() {
            return Ok(default);
        }
        Ok(matches!(answer.as_str(), "y" | "yes"))
    }
}

/// Scripted prompt for unit tests.
#[cfg(any(test, feature = "testing"))]
#[derive(Debug, Default)]
pub struct MockPrompt {
    pub script: std::collections::VecDeque<String>,
    pub confirms: std::collections::VecDeque<bool>,
    pub transcript: Vec<String>,
}

#[cfg(any(test, feature = "testing"))]
impl MockPrompt {
    pub fn with_answers<I, S>(answers: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            script: answers.into_iter().map(Into::into).collect(),
            ..Default::default()
        }
    }

    #[must_use]
    pub fn with_confirms<I>(mut self, confirms: I) -> Self
    where
        I: IntoIterator<Item = bool>,
    {
        self.confirms = confirms.into_iter().collect();
        self
    }
}

#[cfg(any(test, feature = "testing"))]
impl Prompt for MockPrompt {
    fn say(&mut self, line: &str) -> Result<(), Error> {
        self.transcript.push(format!("SAY {line}"));
        Ok(())
    }

    fn ask(&mut self, q: &str, default: Option<&str>) -> Result<String, Error> {
        self.transcript.push(format!("ASK {q}"));
        Ok(self
            .script
            .pop_front()
            .unwrap_or_else(|| default.unwrap_or("").to_string()))
    }

    fn ask_secret(&mut self, q: &str) -> Result<String, Error> {
        self.transcript.push(format!("SECRET {q}"));
        Ok(self.script.pop_front().unwrap_or_default())
    }

    fn confirm(&mut self, q: &str, default: bool) -> Result<bool, Error> {
        self.transcript.push(format!("CONFIRM {q}"));
        Ok(self.confirms.pop_front().unwrap_or(default))
    }
}
