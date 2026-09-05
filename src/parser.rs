use crate::ast::*;
use crate::diagnostic::Diagnostic;
use crate::token::{Token, TokenKind};

pub fn parse(tokens: Vec<Token>) -> Result<Program, Diagnostic> {
    Parser { tokens, current: 0 }.program()
}

struct Parser {
    tokens: Vec<Token>,
    current: usize,
}

impl Parser {
    fn program(&mut self) -> Result<Program, Diagnostic> {
        self.expect_simple(TokenKind::Module, "expected `module`")?;
        let mut module = self.identifier("expected module name")?;
        while self.take_simple(TokenKind::Dot) {
            module.push('.');
            module.push_str(&self.identifier("expected module name after `.`")?);
        }
        self.expect_simple(
            TokenKind::Semicolon,
            "expected `;` after module declaration",
        )?;
        let mut functions = Vec::new();
        while !self.check(&TokenKind::Eof) {
            functions.push(self.function()?);
        }
        Ok(Program { module, functions })
    }

    fn function(&mut self) -> Result<Function, Diagnostic> {
        self.expect_simple(TokenKind::Func, "expected `func`")?;
        let name = self.identifier("expected function name")?;
        self.expect_simple(TokenKind::LeftParen, "expected `(`")?;
        let mut parameters = Vec::new();
        if !self.check(&TokenKind::RightParen) {
            loop {
                let mutable = if self.take_simple(TokenKind::Mut) {
                    true
                } else {
                    self.expect_simple(TokenKind::Let, "parameters begin with `let` or `mut`")?;
                    false
                };
                let name = self.identifier("expected parameter name")?;
                self.expect_simple(TokenKind::ColonColon, "expected `::` before parameter type")?;
                let ty = self.ty()?;
                parameters.push(Parameter { name, mutable, ty });
                if !self.take_simple(TokenKind::Comma) {
                    break;
                }
            }
        }
        self.expect_simple(TokenKind::RightParen, "expected `)`")?;
        self.expect_simple(TokenKind::ColonColon, "expected `::` before return type")?;
        let return_type = self.ty()?;
        let body = self.block()?;
        Ok(Function {
            name,
            parameters,
            return_type,
            body,
        })
    }

    fn block(&mut self) -> Result<Vec<Statement>, Diagnostic> {
        self.expect_simple(TokenKind::LeftBrace, "expected `{`")?;
        let mut statements = Vec::new();
        while !self.check(&TokenKind::RightBrace) {
            if self.check(&TokenKind::Eof) {
                return Err(self.error("expected `}`"));
            }
            statements.push(self.statement()?);
        }
        self.advance();
        Ok(statements)
    }

    fn statement(&mut self) -> Result<Statement, Diagnostic> {
        if self.take_simple(TokenKind::Let) {
            return self.binding(false);
        }
        if self.take_simple(TokenKind::Mut) {
            return self.binding(true);
        }
        if self.take_simple(TokenKind::Return) {
            let value = if self.check(&TokenKind::Semicolon) {
                None
            } else {
                Some(self.expression()?)
            };
            self.expect_simple(TokenKind::Semicolon, "expected `;` after return")?;
            return Ok(Statement::Return(value));
        }
        if self.take_simple(TokenKind::If) {
            let condition = self.expression()?;
            let then_body = self.block()?;
            let else_body = if self.take_simple(TokenKind::Else) {
                self.block()?
            } else {
                Vec::new()
            };
            return Ok(Statement::If {
                condition,
                then_body,
                else_body,
            });
        }
        match self.peek().kind.clone() {
            TokenKind::Identifier(name)
                if self
                    .peek_n(1)
                    .is_some_and(|t| matches!(t.kind, TokenKind::Equal)) =>
            {
                self.advance();
                self.advance();
                let value = self.expression()?;
                self.expect_simple(TokenKind::Semicolon, "expected `;` after assignment")?;
                return Ok(Statement::Assign { name, value });
            }
            _ => {}
        }
        let expression = self.expression()?;
        self.expect_simple(TokenKind::Semicolon, "expected `;` after expression")?;
        Ok(Statement::Expression(expression))
    }

    fn binding(&mut self, mutable: bool) -> Result<Statement, Diagnostic> {
        let name = self.identifier("expected variable name")?;
        let ty = if self.take_simple(TokenKind::ColonColon) {
            Some(self.ty()?)
        } else {
            None
        };
        self.expect_simple(TokenKind::Equal, "expected `=` in binding")?;
        let value = self.expression()?;
        self.expect_simple(TokenKind::Semicolon, "expected `;` after binding")?;
        Ok(Statement::Bind {
            name,
            mutable,
            ty,
            value,
        })
    }

    fn expression(&mut self) -> Result<Expression, Diagnostic> {
        self.equality()
    }
    fn equality(&mut self) -> Result<Expression, Diagnostic> {
        let mut expr = self.comparison()?;
        while let Some(op) = self.take_binary(&[
            (TokenKind::EqualEqual, BinaryOperator::Equal),
            (TokenKind::BangEqual, BinaryOperator::NotEqual),
        ]) {
            expr = Expression::Binary {
                left: Box::new(expr),
                operator: op,
                right: Box::new(self.comparison()?),
            };
        }
        Ok(expr)
    }
    fn comparison(&mut self) -> Result<Expression, Diagnostic> {
        let mut expr = self.term()?;
        while let Some(op) = self.take_binary(&[
            (TokenKind::Less, BinaryOperator::Less),
            (TokenKind::LessEqual, BinaryOperator::LessEqual),
            (TokenKind::Greater, BinaryOperator::Greater),
            (TokenKind::GreaterEqual, BinaryOperator::GreaterEqual),
        ]) {
            expr = Expression::Binary {
                left: Box::new(expr),
                operator: op,
                right: Box::new(self.term()?),
            };
        }
        Ok(expr)
    }
    fn term(&mut self) -> Result<Expression, Diagnostic> {
        let mut expr = self.factor()?;
        while let Some(op) = self.take_binary(&[
            (TokenKind::Plus, BinaryOperator::Add),
            (TokenKind::Minus, BinaryOperator::Subtract),
        ]) {
            expr = Expression::Binary {
                left: Box::new(expr),
                operator: op,
                right: Box::new(self.factor()?),
            };
        }
        Ok(expr)
    }
    fn factor(&mut self) -> Result<Expression, Diagnostic> {
        let mut expr = self.unary()?;
        while let Some(op) = self.take_binary(&[
            (TokenKind::Star, BinaryOperator::Multiply),
            (TokenKind::Slash, BinaryOperator::Divide),
        ]) {
            expr = Expression::Binary {
                left: Box::new(expr),
                operator: op,
                right: Box::new(self.unary()?),
            };
        }
        Ok(expr)
    }
    fn unary(&mut self) -> Result<Expression, Diagnostic> {
        if self.take_simple(TokenKind::Minus) {
            return Ok(Expression::Unary {
                operator: UnaryOperator::Negate,
                operand: Box::new(self.unary()?),
            });
        }
        if self.take_simple(TokenKind::Bang) {
            return Ok(Expression::Unary {
                operator: UnaryOperator::Not,
                operand: Box::new(self.unary()?),
            });
        }
        self.call()
    }
    fn call(&mut self) -> Result<Expression, Diagnostic> {
        let mut expr = self.primary()?;
        while self.take_simple(TokenKind::LeftParen) {
            let name = match expr {
                Expression::Variable(name) => name,
                _ => return Err(self.error("only named functions can be called")),
            };
            let mut arguments = Vec::new();
            if !self.check(&TokenKind::RightParen) {
                loop {
                    arguments.push(self.expression()?);
                    if !self.take_simple(TokenKind::Comma) {
                        break;
                    }
                }
            }
            self.expect_simple(TokenKind::RightParen, "expected `)` after arguments")?;
            expr = Expression::Call { name, arguments };
        }
        Ok(expr)
    }
    fn primary(&mut self) -> Result<Expression, Diagnostic> {
        let token = self.advance().clone();
        match token.kind {
            TokenKind::Integer(value) => Ok(Expression::Integer(value)),
            TokenKind::String(value) => Ok(Expression::String(value)),
            TokenKind::True => Ok(Expression::Bool(true)),
            TokenKind::False => Ok(Expression::Bool(false)),
            TokenKind::Identifier(name) => Ok(Expression::Variable(name)),
            TokenKind::LeftParen => {
                let expr = self.expression()?;
                self.expect_simple(TokenKind::RightParen, "expected `)`")?;
                Ok(expr)
            }
            _ => Err(Diagnostic::new(
                "expected expression",
                token.line,
                token.column,
            )),
        }
    }

    fn ty(&mut self) -> Result<Type, Diagnostic> {
        let token = self.advance().clone();
        match token.kind {
            TokenKind::Identifier(name) => match name.as_str() {
                "int" => Ok(Type::Int),
                "bool" => Ok(Type::Bool),
                "string" => Ok(Type::String),
                "void" => Ok(Type::Void),
                _ => Err(Diagnostic::new(
                    format!("unknown type `{name}`"),
                    token.line,
                    token.column,
                )),
            },
            _ => Err(Diagnostic::new("expected type", token.line, token.column)),
        }
    }
    fn take_binary(&mut self, choices: &[(TokenKind, BinaryOperator)]) -> Option<BinaryOperator> {
        for (kind, op) in choices {
            if self.check(kind) {
                self.advance();
                return Some(*op);
            }
        }
        None
    }
    fn identifier(&mut self, message: &str) -> Result<String, Diagnostic> {
        let token = self.advance().clone();
        if let TokenKind::Identifier(name) = token.kind {
            Ok(name)
        } else {
            Err(Diagnostic::new(message, token.line, token.column))
        }
    }
    fn expect_simple(&mut self, kind: TokenKind, message: &str) -> Result<(), Diagnostic> {
        if self.take_simple(kind) {
            Ok(())
        } else {
            Err(self.error(message))
        }
    }
    fn take_simple(&mut self, kind: TokenKind) -> bool {
        if self.check(&kind) {
            self.advance();
            true
        } else {
            false
        }
    }
    fn check(&self, kind: &TokenKind) -> bool {
        std::mem::discriminant(&self.peek().kind) == std::mem::discriminant(kind)
    }
    fn advance(&mut self) -> &Token {
        let index = self.current;
        if !self.check(&TokenKind::Eof) {
            self.current += 1;
        }
        &self.tokens[index]
    }
    fn peek(&self) -> &Token {
        &self.tokens[self.current]
    }
    fn peek_n(&self, n: usize) -> Option<&Token> {
        self.tokens.get(self.current + n)
    }
    fn error(&self, message: impl Into<String>) -> Diagnostic {
        Diagnostic::new(message, self.peek().line, self.peek().column)
    }
}
