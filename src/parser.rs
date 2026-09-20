use crate::ast::*;
use crate::diagnostic::Diagnostic;
use crate::token::{Token, TokenKind};

pub fn parse(tokens: Vec<Token>) -> Result<Program, Diagnostic> {
    Parser {
        tokens,
        current: 0,
        loop_depth: 0,
    }
    .program()
}

struct Parser {
    tokens: Vec<Token>,
    current: usize,
    loop_depth: usize,
}

impl Parser {
    fn program(&mut self) -> Result<Program, Diagnostic> {
        self.expect_simple(TokenKind::Module, "expected `module`")?;
        let module = self.dotted_name("expected module name")?;
        self.expect_simple(
            TokenKind::Semicolon,
            "expected `;` after module declaration",
        )?;
        let mut uses = Vec::new();
        while self.take_simple(TokenKind::Use) {
            uses.push(self.dotted_name("expected module name after `@use`")?);
            self.expect_simple(TokenKind::Semicolon, "expected `;` after `@use`")?;
        }
        let mut packs = Vec::new();
        let mut functions = Vec::new();
        while !self.check(&TokenKind::Eof) {
            if self.check(&TokenKind::Use) {
                return Err(self.error("`@use` must come before any function"));
            }
            if self.take_simple(TokenKind::Pack) {
                let pack = self.pack()?;
                if packs
                    .iter()
                    .any(|existing: &Pack| existing.name == pack.name)
                {
                    return Err(self.error(format!("duplicate pack `{}`", pack.name)));
                }
                packs.push(pack);
            } else {
                functions.push(self.function()?);
            }
        }
        Ok(Program {
            module,
            uses,
            packs,
            functions,
        })
    }

    /// Parses one product-type declaration after its `pack` keyword.
    fn pack(&mut self) -> Result<Pack, Diagnostic> {
        let name = self.identifier("expected pack name")?;
        if !name.chars().next().is_some_and(char::is_uppercase) {
            return Err(self.error("pack names must begin with an uppercase letter"));
        }
        self.expect_simple(TokenKind::LeftBrace, "expected `{` after pack name")?;
        let mut fields = Vec::new();
        while !self.check(&TokenKind::RightBrace) {
            let field = self.identifier("expected field name")?;
            self.expect_simple(TokenKind::ColonColon, "expected `::` before field type")?;
            let ty = self.ty()?;
            self.expect_simple(TokenKind::Semicolon, "expected `;` after pack field")?;
            if fields
                .iter()
                .any(|existing: &PackField| existing.name == field)
            {
                return Err(self.error(format!("duplicate field `{field}` in pack `{name}`")));
            }
            fields.push(PackField { name: field, ty });
        }
        self.advance();
        Ok(Pack { name, fields })
    }

    fn dotted_name(&mut self, message: &str) -> Result<String, Diagnostic> {
        let mut name = self.identifier(message)?;
        while self.take_simple(TokenKind::Dot) {
            name.push('.');
            name.push_str(&self.identifier("expected name after `.`")?);
        }
        Ok(name)
    }

    fn function(&mut self) -> Result<Function, Diagnostic> {
        let public = self.take_simple(TokenKind::Pub);
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
            public,
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
        if self.take_simple(TokenKind::For) {
            let name = self.identifier("expected loop variable after `for`")?;
            self.expect_simple(TokenKind::In, "expected `in` after loop variable")?;
            let first = self.expression()?;
            let iterable = if self.take_simple(TokenKind::DotDot) {
                let end = self.expression()?;
                let step = if self.take_simple(TokenKind::By) {
                    Some(self.expression()?)
                } else {
                    None
                };
                Iterable::Range {
                    start: first,
                    end,
                    step,
                }
            } else {
                Iterable::List(first)
            };
            self.loop_depth += 1;
            let body = self.block()?;
            self.loop_depth -= 1;
            return Ok(Statement::For {
                name,
                iterable,
                body,
            });
        }
        if self.take_simple(TokenKind::While) {
            let condition = self.expression()?;
            self.loop_depth += 1;
            let body = self.block()?;
            self.loop_depth -= 1;
            return Ok(Statement::While { condition, body });
        }
        if self.take_simple(TokenKind::Loop) {
            self.loop_depth += 1;
            let body = self.block()?;
            self.loop_depth -= 1;
            return Ok(Statement::Loop { body });
        }
        if self.take_simple(TokenKind::Break) {
            if self.loop_depth == 0 {
                return Err(self.error("`break` may only be used inside a loop"));
            }
            self.expect_simple(TokenKind::Semicolon, "expected `;` after `break`")?;
            return Ok(Statement::Break);
        }
        if self.take_simple(TokenKind::Skip) {
            if self.loop_depth == 0 {
                return Err(self.error("`skip` may only be used inside a loop"));
            }
            self.expect_simple(TokenKind::Semicolon, "expected `;` after `skip`")?;
            return Ok(Statement::Skip);
        }
        if self.take_simple(TokenKind::Ask) {
            let (value, else_body) = self.ask_tail()?;
            return Ok(Statement::Ask {
                binding: None,
                value,
                else_body,
            });
        }
        if self.take_simple(TokenKind::If) {
            return self.if_statement();
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

    fn if_statement(&mut self) -> Result<Statement, Diagnostic> {
        let condition = self.expression()?;
        let then_body = self.block()?;
        let else_body = if self.take_simple(TokenKind::Else) {
            if self.take_simple(TokenKind::If) {
                vec![self.if_statement()?]
            } else {
                self.block()?
            }
        } else {
            Vec::new()
        };
        Ok(Statement::If {
            condition,
            then_body,
            else_body,
        })
    }

    fn binding(&mut self, mutable: bool) -> Result<Statement, Diagnostic> {
        let name = self.identifier("expected variable name")?;
        let ty = if self.take_simple(TokenKind::ColonColon) {
            Some(self.ty()?)
        } else {
            None
        };
        self.expect_simple(TokenKind::Equal, "expected `=` in binding")?;
        if self.take_simple(TokenKind::Ask) {
            let (value, else_body) = self.ask_tail()?;
            return Ok(Statement::Ask {
                binding: Some(AskBinding { name, mutable, ty }),
                value,
                else_body,
            });
        }
        let value = self.expression()?;
        self.expect_simple(TokenKind::Semicolon, "expected `;` after binding")?;
        Ok(Statement::Bind {
            name,
            mutable,
            ty,
            value,
        })
    }

    /// `Expr else { ... };` after `ask`
    fn ask_tail(&mut self) -> Result<(Expression, Vec<Statement>), Diagnostic> {
        let value = self.expression()?;
        self.expect_simple(TokenKind::Else, "expected `else` after `ask` expression")?;
        let else_body = self.block()?;
        self.expect_simple(
            TokenKind::Semicolon,
            "expected `;` after `ask ... else { }`",
        )?;
        Ok((value, else_body))
    }

    fn expression(&mut self) -> Result<Expression, Diagnostic> {
        self.or()
    }
    fn or(&mut self) -> Result<Expression, Diagnostic> {
        let mut expr = self.and()?;
        while self.take_simple(TokenKind::PipePipe) {
            expr = Expression::Logical {
                left: Box::new(expr),
                operator: LogicalOperator::Or,
                right: Box::new(self.and()?),
            };
        }
        Ok(expr)
    }
    fn and(&mut self) -> Result<Expression, Diagnostic> {
        let mut expr = self.equality()?;
        while self.take_simple(TokenKind::AmpAmp) {
            expr = Expression::Logical {
                left: Box::new(expr),
                operator: LogicalOperator::And,
                right: Box::new(self.equality()?),
            };
        }
        Ok(expr)
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
            (TokenKind::Percent, BinaryOperator::Remainder),
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
        loop {
            if self.take_simple(TokenKind::LeftBracket) {
                let index = self.expression()?;
                self.expect_simple(TokenKind::RightBracket, "expected `]` after index")?;
                expr = Expression::Index {
                    target: Box::new(expr),
                    index: Box::new(index),
                };
                continue;
            }
            if !self.take_simple(TokenKind::LeftParen) {
                break;
            }
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
            TokenKind::Float(value) => Ok(Expression::Float(value)),
            TokenKind::String(value) => Ok(Expression::String(value)),
            TokenKind::True => Ok(Expression::Bool(true)),
            TokenKind::False => Ok(Expression::Bool(false)),
            TokenKind::Identifier(mut name) => {
                while self.take_simple(TokenKind::Dot) {
                    name.push('.');
                    name.push_str(&self.identifier("expected name after `.`")?);
                }
                if name.chars().next().is_some_and(char::is_uppercase)
                    && self.take_simple(TokenKind::LeftBrace)
                {
                    let mut fields = Vec::new();
                    while !self.check(&TokenKind::RightBrace) {
                        let field = self.identifier("expected field name")?;
                        self.expect_simple(TokenKind::Equal, "expected `=` after field name")?;
                        let value = self.expression()?;
                        if fields.iter().any(|(existing, _)| existing == &field) {
                            return Err(self.error(format!("duplicate field `{field}`")));
                        }
                        fields.push((field, value));
                        if !self.take_simple(TokenKind::Comma) {
                            break;
                        }
                    }
                    self.expect_simple(TokenKind::RightBrace, "expected `}` after pack value")?;
                    Ok(Expression::Pack { name, fields })
                } else {
                    Ok(Expression::Variable(name))
                }
            }
            TokenKind::LeftBracket => {
                let mut items = Vec::new();
                if !self.check(&TokenKind::RightBracket) {
                    loop {
                        items.push(self.expression()?);
                        if !self.take_simple(TokenKind::Comma) {
                            break;
                        }
                    }
                }
                self.expect_simple(TokenKind::RightBracket, "expected `]` after list items")?;
                Ok(Expression::List(items))
            }
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
                "float" => Ok(Type::Float),
                "bool" => Ok(Type::Bool),
                "string" => Ok(Type::String),
                "void" => Ok(Type::Void),
                "list" => {
                    self.expect_simple(TokenKind::Less, "expected `<` after `list`")?;
                    let element = self.ty()?;
                    self.expect_simple(TokenKind::Greater, "expected `>` after list element type")?;
                    Ok(Type::List(Box::new(element)))
                }
                _ => Ok(Type::Named(name)),
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
