#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    pub module: String,
    /// Modules named by `@use A.B;`, in source order.
    pub uses: Vec<String>,
    pub globals: Vec<Global>,
    pub packs: Vec<Pack>,
    pub functions: Vec<Function>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Global {
    pub name: String,
    pub ty: Option<Type>,
    pub value: Expression,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Pack {
    pub name: String,
    pub fields: Vec<PackField>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PackField {
    pub name: String,
    pub ty: Type,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Function {
    pub name: String,
    /// `pub func`: callable from other modules. Functions are private by default.
    pub public: bool,
    pub parameters: Vec<Parameter>,
    pub return_type: Type,
    pub body: Vec<Statement>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Parameter {
    pub name: String,
    pub mutable: bool,
    pub ty: Type,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    Int,
    Float,
    Bool,
    String,
    Void,
    /// `list<T>`
    List(Box<Type>),
    Named(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Statement {
    Bind {
        name: String,
        mutable: bool,
        ty: Option<Type>,
        value: Expression,
    },
    Assign {
        name: String,
        value: Expression,
    },
    Return(Option<Expression>),
    If {
        condition: Expression,
        then_body: Vec<Statement>,
        else_body: Vec<Statement>,
    },
    /// `for name in 0..10 { }` or `for name in list { }`
    For {
        name: String,
        iterable: Iterable,
        body: Vec<Statement>,
    },
    While {
        condition: Expression,
        body: Vec<Statement>,
    },
    Loop {
        body: Vec<Statement>,
    },
    Break,
    Skip,
    /// `let name = ask Expr else { };` (binding: the else block must return or exit)
    /// or `ask Expr else { };` (statement: the else block may continue).
    Ask {
        binding: Option<AskBinding>,
        value: Expression,
        else_body: Vec<Statement>,
    },
    Expression(Expression),
}

#[derive(Debug, Clone, PartialEq)]
pub struct AskBinding {
    pub name: String,
    pub mutable: bool,
    pub ty: Option<Type>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Iterable {
    /// Inclusive `start..end`, optionally with a positive `by` step magnitude.
    Range {
        start: Expression,
        end: Expression,
        step: Option<Expression>,
    },
    List(Expression),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expression {
    Integer(i64),
    Float(f64),
    Bool(bool),
    String(String),
    List(Vec<Expression>),
    Variable(String),
    Pack {
        name: String,
        fields: Vec<(String, Expression)>,
    },
    Unary {
        operator: UnaryOperator,
        operand: Box<Expression>,
    },
    Binary {
        left: Box<Expression>,
        operator: BinaryOperator,
        right: Box<Expression>,
    },
    /// `&&` and `||`, which only evaluate the right side when needed.
    Logical {
        left: Box<Expression>,
        operator: LogicalOperator,
        right: Box<Expression>,
    },
    Index {
        target: Box<Expression>,
        index: Box<Expression>,
    },
    Call {
        name: String,
        arguments: Vec<Expression>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UnaryOperator {
    Negate,
    Not,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BinaryOperator {
    Add,
    Subtract,
    Multiply,
    Divide,
    Remainder,
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LogicalOperator {
    And,
    Or,
}
