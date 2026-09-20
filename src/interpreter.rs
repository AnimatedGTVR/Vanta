use std::collections::HashMap;
use std::fmt;

use crate::ast::*;
use crate::diagnostic::Diagnostic;

const MAX_CALL_DEPTH: usize = 256;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Int(i64),
    Bool(bool),
    String(String),
    Void,
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Int(v) => write!(f, "{v}"),
            Value::Bool(v) => write!(f, "{v}"),
            Value::String(v) => write!(f, "{v}"),
            Value::Void => Ok(()),
        }
    }
}

#[derive(Clone)]
struct Binding {
    value: Value,
    mutable: bool,
}

pub fn interpret(program: &Program) -> Result<String, Diagnostic> {
    let mut interpreter = Interpreter {
        program,
        output: String::new(),
        call_depth: 0,
    };
    if !program.functions.iter().any(|f| f.name == "Start") {
        return Err(runtime("entry function `Start` was not found"));
    }
    interpreter.call("Start", Vec::new())?;
    Ok(interpreter.output)
}

struct Interpreter<'a> {
    program: &'a Program,
    output: String,
    call_depth: usize,
}

impl Interpreter<'_> {
    fn call(&mut self, name: &str, arguments: Vec<Value>) -> Result<Value, Diagnostic> {
        if self.call_depth >= MAX_CALL_DEPTH {
            return Err(runtime(format!(
                "maximum call depth of {MAX_CALL_DEPTH} exceeded"
            )));
        }
        self.call_depth += 1;
        let result = self.call_inner(name, arguments);
        self.call_depth -= 1;
        result
    }

    fn call_inner(&mut self, name: &str, arguments: Vec<Value>) -> Result<Value, Diagnostic> {
        if name == "emit" {
            if arguments.len() != 1 {
                return Err(runtime("`emit` expects exactly one argument"));
            }
            self.output.push_str(&arguments[0].to_string());
            self.output.push('\n');
            return Ok(Value::Void);
        }
        let function = self
            .program
            .functions
            .iter()
            .find(|f| f.name == name)
            .cloned()
            .ok_or_else(|| runtime(format!("unknown function `{name}`")))?;
        if function.parameters.len() != arguments.len() {
            return Err(runtime(format!(
                "`{name}` expects {} argument(s), received {}",
                function.parameters.len(),
                arguments.len()
            )));
        }
        let mut environment = HashMap::new();
        for (parameter, value) in function.parameters.iter().zip(arguments) {
            ensure_type(&value, &parameter.ty)?;
            environment.insert(
                parameter.name.clone(),
                Binding {
                    value,
                    mutable: parameter.mutable,
                },
            );
        }
        let result = self
            .execute_block(&function.body, &mut environment)?
            .unwrap_or(Value::Void);
        ensure_type(&result, &function.return_type)?;
        Ok(result)
    }

    fn execute_block(
        &mut self,
        statements: &[Statement],
        env: &mut HashMap<String, Binding>,
    ) -> Result<Option<Value>, Diagnostic> {
        for statement in statements {
            match statement {
                Statement::Bind {
                    name,
                    mutable,
                    ty,
                    value,
                } => {
                    if env.contains_key(name) {
                        return Err(runtime(format!("`{name}` is already defined")));
                    }
                    let value = self.evaluate(value, env)?;
                    if let Some(ty) = ty {
                        ensure_type(&value, ty)?;
                    }
                    env.insert(
                        name.clone(),
                        Binding {
                            value,
                            mutable: *mutable,
                        },
                    );
                }
                Statement::Assign { name, value } => {
                    let value = self.evaluate(value, env)?;
                    let binding = env
                        .get_mut(name)
                        .ok_or_else(|| runtime(format!("unknown variable `{name}`")))?;
                    if !binding.mutable {
                        return Err(runtime(format!(
                            "cannot assign to immutable variable `{name}`"
                        )));
                    }
                    if std::mem::discriminant(&binding.value) != std::mem::discriminant(&value) {
                        return Err(runtime("assignment cannot change a variable's type"));
                    }
                    binding.value = value;
                }
                Statement::Return(expression) => {
                    return Ok(Some(match expression {
                        Some(expr) => self.evaluate(expr, env)?,
                        None => Value::Void,
                    }));
                }
                Statement::If {
                    condition,
                    then_body,
                    else_body,
                } => {
                    let condition = self.evaluate(condition, env)?;
                    let branch = match condition {
                        Value::Bool(true) => then_body,
                        Value::Bool(false) => else_body,
                        _ => return Err(runtime("if condition must be `bool`")),
                    };
                    if let Some(value) = self.execute_block(branch, env)? {
                        return Ok(Some(value));
                    }
                }
                Statement::Expression(expression) => {
                    self.evaluate(expression, env)?;
                }
            }
        }
        Ok(None)
    }

    fn evaluate(
        &mut self,
        expression: &Expression,
        env: &HashMap<String, Binding>,
    ) -> Result<Value, Diagnostic> {
        match expression {
            Expression::Integer(v) => Ok(Value::Int(*v)),
            Expression::Bool(v) => Ok(Value::Bool(*v)),
            Expression::String(v) => Ok(Value::String(interpolate(v, env)?)),
            Expression::Variable(name) => env
                .get(name)
                .map(|b| b.value.clone())
                .ok_or_else(|| runtime(format!("unknown variable `{name}`"))),
            Expression::Unary { operator, operand } => {
                let value = self.evaluate(operand, env)?;
                match (operator, value) {
                    (UnaryOperator::Negate, Value::Int(v)) => v
                        .checked_neg()
                        .map(Value::Int)
                        .ok_or_else(|| runtime("integer overflow")),
                    (UnaryOperator::Not, Value::Bool(v)) => Ok(Value::Bool(!v)),
                    _ => Err(runtime("invalid unary operation")),
                }
            }
            Expression::Binary {
                left,
                operator,
                right,
            } => {
                let left = self.evaluate(left, env)?;
                let right = self.evaluate(right, env)?;
                binary(left, *operator, right)
            }
            Expression::Call { name, arguments } => {
                let values = arguments
                    .iter()
                    .map(|arg| self.evaluate(arg, env))
                    .collect::<Result<Vec<_>, _>>()?;
                self.call(name, values)
            }
        }
    }
}

fn interpolate(text: &str, env: &HashMap<String, Binding>) -> Result<String, Diagnostic> {
    let mut output = String::new();
    let mut rest = text;
    while let Some(start) = rest.find('{') {
        output.push_str(&rest[..start]);
        let tail = &rest[start + 1..];
        let end = tail
            .find('}')
            .ok_or_else(|| runtime("unclosed interpolation in string"))?;
        let name = &tail[..end];
        let value = env
            .get(name)
            .ok_or_else(|| runtime(format!("unknown interpolation variable `{name}`")))?;
        output.push_str(&value.value.to_string());
        rest = &tail[end + 1..];
    }
    output.push_str(rest);
    Ok(output)
}

fn binary(left: Value, operator: BinaryOperator, right: Value) -> Result<Value, Diagnostic> {
    use BinaryOperator::*;
    match (left, operator, right) {
        (Value::Int(a), Add, Value::Int(b)) => checked_integer(a.checked_add(b)),
        (Value::String(a), Add, Value::String(b)) => Ok(Value::String(a + &b)),
        (Value::Int(a), Subtract, Value::Int(b)) => checked_integer(a.checked_sub(b)),
        (Value::Int(a), Multiply, Value::Int(b)) => checked_integer(a.checked_mul(b)),
        (Value::Int(_), Divide, Value::Int(0)) => Err(runtime("division by zero")),
        (Value::Int(a), Divide, Value::Int(b)) => checked_integer(a.checked_div(b)),
        (a, Equal, b) => Ok(Value::Bool(a == b)),
        (a, NotEqual, b) => Ok(Value::Bool(a != b)),
        (Value::Int(a), Less, Value::Int(b)) => Ok(Value::Bool(a < b)),
        (Value::Int(a), LessEqual, Value::Int(b)) => Ok(Value::Bool(a <= b)),
        (Value::Int(a), Greater, Value::Int(b)) => Ok(Value::Bool(a > b)),
        (Value::Int(a), GreaterEqual, Value::Int(b)) => Ok(Value::Bool(a >= b)),
        _ => Err(runtime("invalid binary operation")),
    }
}

fn checked_integer(value: Option<i64>) -> Result<Value, Diagnostic> {
    value
        .map(Value::Int)
        .ok_or_else(|| runtime("integer overflow"))
}

fn ensure_type(value: &Value, ty: &Type) -> Result<(), Diagnostic> {
    let valid = matches!(
        (value, ty),
        (Value::Int(_), Type::Int)
            | (Value::Bool(_), Type::Bool)
            | (Value::String(_), Type::String)
            | (Value::Void, Type::Void)
    );
    if valid {
        Ok(())
    } else {
        Err(runtime("value does not match declared type"))
    }
}

fn runtime(message: impl Into<String>) -> Diagnostic {
    Diagnostic::new(message, 0, 0)
}
