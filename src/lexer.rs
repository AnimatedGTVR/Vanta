use crate::diagnostic::Diagnostic;
use crate::token::{Token, TokenKind};

pub fn lex(source: &str) -> Result<Vec<Token>, Diagnostic> {
    Lexer::new(source).scan_tokens()
}

struct Lexer {
    chars: Vec<char>,
    current: usize,
    line: usize,
    column: usize,
}

impl Lexer {
    fn new(source: &str) -> Self {
        Self {
            chars: source.chars().collect(),
            current: 0,
            line: 1,
            column: 1,
        }
    }

    fn scan_tokens(mut self) -> Result<Vec<Token>, Diagnostic> {
        let mut tokens = Vec::new();
        while !self.at_end() {
            self.skip_whitespace_and_comments()?;
            if self.at_end() {
                break;
            }
            tokens.push(self.scan_token()?);
        }
        tokens.push(Token {
            kind: TokenKind::Eof,
            line: self.line,
            column: self.column,
        });
        Ok(tokens)
    }

    fn scan_token(&mut self) -> Result<Token, Diagnostic> {
        let line = self.line;
        let column = self.column;
        let ch = self.advance();
        let kind = match ch {
            '(' => TokenKind::LeftParen,
            ')' => TokenKind::RightParen,
            '{' => TokenKind::LeftBrace,
            '}' => TokenKind::RightBrace,
            ',' => TokenKind::Comma,
            ';' => TokenKind::Semicolon,
            '.' => TokenKind::Dot,
            '+' => TokenKind::Plus,
            '-' => TokenKind::Minus,
            '*' => TokenKind::Star,
            '/' => TokenKind::Slash,
            ':' if self.take(':') => TokenKind::ColonColon,
            '=' if self.take('=') => TokenKind::EqualEqual,
            '=' => TokenKind::Equal,
            '!' if self.take('=') => TokenKind::BangEqual,
            '!' => TokenKind::Bang,
            '<' if self.take('=') => TokenKind::LessEqual,
            '<' => TokenKind::Less,
            '>' if self.take('=') => TokenKind::GreaterEqual,
            '>' => TokenKind::Greater,
            '"' => return self.string(line, column),
            c if c.is_ascii_digit() => return self.number(c, line, column),
            c if is_ident_start(c) => return Ok(self.identifier(c, line, column)),
            _ => {
                return Err(Diagnostic::new(
                    format!("unexpected character `{ch}`"),
                    line,
                    column,
                ));
            }
        };
        Ok(Token { kind, line, column })
    }

    fn identifier(&mut self, first: char, line: usize, column: usize) -> Token {
        let mut text = String::from(first);
        while self.peek().is_some_and(is_ident_continue) {
            text.push(self.advance());
        }
        let kind = match text.as_str() {
            "module" => TokenKind::Module,
            "func" => TokenKind::Func,
            "let" => TokenKind::Let,
            "mut" => TokenKind::Mut,
            "return" => TokenKind::Return,
            "if" => TokenKind::If,
            "else" => TokenKind::Else,
            "true" => TokenKind::True,
            "false" => TokenKind::False,
            _ => TokenKind::Identifier(text),
        };
        Token { kind, line, column }
    }

    fn number(&mut self, first: char, line: usize, column: usize) -> Result<Token, Diagnostic> {
        let mut text = String::from(first);
        while self.peek().is_some_and(|c| c.is_ascii_digit()) {
            text.push(self.advance());
        }
        let value = text
            .parse::<i64>()
            .map_err(|_| Diagnostic::new("integer literal out of range", line, column))?;
        Ok(Token {
            kind: TokenKind::Integer(value),
            line,
            column,
        })
    }

    fn string(&mut self, line: usize, column: usize) -> Result<Token, Diagnostic> {
        let mut value = String::new();
        while let Some(ch) = self.peek() {
            if ch == '"' {
                self.advance();
                return Ok(Token {
                    kind: TokenKind::String(value),
                    line,
                    column,
                });
            }
            if ch == '\n' {
                return Err(Diagnostic::new("unterminated string", line, column));
            }
            if ch == '\\' {
                self.advance();
                let escaped = self.peek().ok_or_else(|| {
                    Diagnostic::new("unterminated escape", self.line, self.column)
                })?;
                self.advance();
                value.push(match escaped {
                    'n' => '\n',
                    't' => '\t',
                    '"' => '"',
                    '\\' => '\\',
                    other => other,
                });
            } else {
                value.push(self.advance());
            }
        }
        Err(Diagnostic::new("unterminated string", line, column))
    }

    fn skip_whitespace_and_comments(&mut self) -> Result<(), Diagnostic> {
        loop {
            match self.peek() {
                Some(' ' | '\r' | '\t' | '\n') => {
                    self.advance();
                }
                Some('#') if self.peek_next() == Some('|') => {
                    let line = self.line;
                    let column = self.column;
                    self.advance();
                    self.advance();
                    while !(self.peek() == Some('|') && self.peek_next() == Some('#')) {
                        if self.at_end() {
                            return Err(Diagnostic::new(
                                "unterminated block comment",
                                line,
                                column,
                            ));
                        }
                        self.advance();
                    }
                    self.advance();
                    self.advance();
                }
                Some('#') => {
                    while self.peek().is_some_and(|c| c != '\n') {
                        self.advance();
                    }
                }
                _ => break,
            }
        }
        Ok(())
    }

    fn advance(&mut self) -> char {
        let ch = self.chars[self.current];
        self.current += 1;
        if ch == '\n' {
            self.line += 1;
            self.column = 1;
        } else {
            self.column += 1;
        }
        ch
    }
    fn take(&mut self, expected: char) -> bool {
        if self.peek() == Some(expected) {
            self.advance();
            true
        } else {
            false
        }
    }
    fn peek(&self) -> Option<char> {
        self.chars.get(self.current).copied()
    }
    fn peek_next(&self) -> Option<char> {
        self.chars.get(self.current + 1).copied()
    }
    fn at_end(&self) -> bool {
        self.current >= self.chars.len()
    }
}

fn is_ident_start(ch: char) -> bool {
    ch.is_ascii_alphabetic() || ch == '_'
}
fn is_ident_continue(ch: char) -> bool {
    is_ident_start(ch) || ch.is_ascii_digit()
}
