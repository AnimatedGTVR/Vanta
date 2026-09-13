use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub struct Diagnostic {
    pub message: String,
    pub line: usize,
    pub column: usize,
    /// A failure of the outside world (a missing file, a failed process, unparsable
    /// input) that a program may handle with `ask ... else`. Everything else is a
    /// program error and always stops the program.
    pub recoverable: bool,
}

impl Diagnostic {
    pub fn new(message: impl Into<String>, line: usize, column: usize) -> Self {
        Self {
            message: message.into(),
            line,
            column,
            recoverable: false,
        }
    }

    /// A recoverable failure; see [`Diagnostic::recoverable`].
    pub fn failure(message: impl Into<String>) -> Self {
        Self {
            recoverable: true,
            ..Self::new(message, 0, 0)
        }
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Error[V0001] at {}:{}: {}",
            self.line, self.column, self.message
        )
    }
}

impl std::error::Error for Diagnostic {}
