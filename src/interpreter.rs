use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::io::Write;

use crate::ast::*;
use crate::builtins;
use crate::diagnostic::Diagnostic;

const MAX_CALL_DEPTH: usize = 256;

/// Namespaces reserved for the standard library; modules may not use them as names.
pub const BUILTIN_NAMESPACES: &[&str] =
    &["String", "List", "Math", "File", "Dir", "Process", "Env"];

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Int(i64),
    Float(f64),
    Bool(bool),
    String(String),
    List(Vec<Value>),
    Pack {
        name: String,
        fields: BTreeMap<String, Value>,
    },
    Void,
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Int(v) => write!(f, "{v}"),
            Value::Float(v) => write!(f, "{v}"),
            Value::Bool(v) => write!(f, "{v}"),
            Value::String(v) => write!(f, "{v}"),
            Value::List(items) => {
                write!(f, "[")?;
                for (index, item) in items.iter().enumerate() {
                    if index > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{item}")?;
                }
                write!(f, "]")
            }
            Value::Pack { name, fields } => {
                write!(f, "{name} {{ ")?;
                for (index, (field, value)) in fields.iter().enumerate() {
                    if index > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{field} = {value}")?;
                }
                write!(f, " }}")
            }
            Value::Void => Ok(()),
        }
    }
}

/// Every module a program loaded, keyed by module name, plus which one to start.
pub struct Modules {
    pub entry: String,
    pub programs: HashMap<String, Program>,
}

/// Where `emit` writes.
pub enum Sink {
    Stdout,
    Buffer(String),
}

pub struct Execution {
    pub exit_code: i32,
    /// What `emit` wrote, when the sink was a buffer.
    pub output: String,
}

/// Stack for the interpreter thread: deep enough that `MAX_CALL_DEPTH` Vanta calls
/// always end in a diagnostic, never a native stack overflow (debug builds included).
const INTERPRETER_STACK_BYTES: usize = 256 * 1024 * 1024;

/// Runs `Start` in the entry module.
pub fn interpret(
    modules: &Modules,
    args: Vec<String>,
    sink: Sink,
) -> Result<Execution, Diagnostic> {
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .name("vanta".into())
            .stack_size(INTERPRETER_STACK_BYTES)
            .spawn_scoped(scope, || interpret_on_this_thread(modules, args, sink))
            .map_err(|cause| runtime(format!("could not start the interpreter: {cause}")))?
            .join()
            .unwrap_or_else(|_| Err(runtime("the interpreter panicked")))
    })
}

fn interpret_on_this_thread(
    modules: &Modules,
    args: Vec<String>,
    sink: Sink,
) -> Result<Execution, Diagnostic> {
    let entry = &modules.programs[&modules.entry];
    if !entry.functions.iter().any(|f| f.name == "Start") {
        return Err(runtime("entry function `Start` was not found"));
    }
    let mut interpreter = Interpreter {
        modules,
        args,
        sink,
        call_depth: 0,
        globals: HashMap::new(),
    };
    interpreter
        .initialize_globals()
        .map_err(|halt| match halt {
            Halt::Error(diagnostic) => diagnostic,
            Halt::Exit(_) => runtime("module binding initialization cannot exit"),
        })?;
    let exit_code = match interpreter.call(&modules.entry, "Start", Vec::new()) {
        Ok(_) => 0,
        Err(Halt::Exit(code)) => code,
        Err(Halt::Error(diagnostic)) => {
            interpreter.flush();
            return Err(diagnostic);
        }
    };
    interpreter.flush();
    let output = match interpreter.sink {
        Sink::Buffer(output) => output,
        Sink::Stdout => String::new(),
    };
    Ok(Execution { exit_code, output })
}

/// Why evaluation stopped early.
enum Halt {
    Error(Diagnostic),
    Exit(i32),
}

impl From<Diagnostic> for Halt {
    fn from(diagnostic: Diagnostic) -> Self {
        Halt::Error(diagnostic)
    }
}

enum Flow {
    Normal,
    Return(Value),
    Break,
    Skip,
}

#[derive(Clone)]
struct Binding {
    value: Value,
    mutable: bool,
}

/// Block scopes of one function call, innermost last. A name may not be rebound
/// while an outer block of the same call still has it.
struct Scopes {
    frames: Vec<HashMap<String, Binding>>,
}

impl Scopes {
    fn new() -> Self {
        Self {
            frames: vec![HashMap::new()],
        }
    }

    fn get(&self, name: &str) -> Option<&Binding> {
        self.frames.iter().rev().find_map(|frame| frame.get(name))
    }

    fn define(&mut self, name: &str, value: Value, mutable: bool) -> Result<(), Diagnostic> {
        if self.get(name).is_some() {
            return Err(runtime(format!("`{name}` is already defined")));
        }
        self.frames
            .last_mut()
            .expect("a call always has a scope")
            .insert(name.to_owned(), Binding { value, mutable });
        Ok(())
    }

    fn assign(&mut self, name: &str, value: Value) -> Result<(), Diagnostic> {
        let binding = self
            .frames
            .iter_mut()
            .rev()
            .find_map(|frame| frame.get_mut(name))
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
        Ok(())
    }
}

struct Interpreter<'a> {
    modules: &'a Modules,
    args: Vec<String>,
    sink: Sink,
    call_depth: usize,
    globals: HashMap<String, Vec<(String, Value)>>,
}

impl<'a> Interpreter<'a> {
    /// Evaluates immutable module bindings once, in source order, before `Start`.
    fn initialize_globals(&mut self) -> Result<(), Halt> {
        let modules = self.modules.programs.keys().cloned().collect::<Vec<_>>();
        for module in modules {
            let definitions = self.modules.programs[&module].globals.clone();
            let mut scopes = Scopes::new();
            let mut values = Vec::new();
            for global in definitions {
                let value = self.evaluate(&module, &global.value, &scopes)?;
                if let Some(ty) = &global.ty {
                    ensure_type(&value, ty)?;
                }
                scopes.define(&global.name, value.clone(), false)?;
                values.push((global.name, value));
            }
            self.globals.insert(module, values);
        }
        Ok(())
    }

    fn flush(&mut self) {
        if let Sink::Stdout = self.sink {
            let _ = std::io::stdout().flush();
        }
    }

    fn call(&mut self, module: &str, name: &str, arguments: Vec<Value>) -> Result<Value, Halt> {
        if self.call_depth >= MAX_CALL_DEPTH {
            return Err(runtime(format!("maximum call depth of {MAX_CALL_DEPTH} exceeded")).into());
        }
        self.call_depth += 1;
        let result = self.call_inner(module, name, arguments);
        self.call_depth -= 1;
        result
    }

    fn call_inner(
        &mut self,
        module: &str,
        name: &str,
        arguments: Vec<Value>,
    ) -> Result<Value, Halt> {
        match name {
            "emit" => {
                if arguments.len() != 1 {
                    return Err(runtime("`emit` expects exactly one argument").into());
                }
                match &mut self.sink {
                    Sink::Stdout => {
                        let mut stdout = std::io::stdout().lock();
                        let _ = writeln!(stdout, "{}", arguments[0]);
                        let _ = stdout.flush();
                    }
                    Sink::Buffer(output) => {
                        output.push_str(&arguments[0].to_string());
                        output.push('\n');
                    }
                }
                return Ok(Value::Void);
            }
            "emitError" => {
                if arguments.len() != 1 {
                    return Err(runtime("`emitError` expects exactly one argument").into());
                }
                self.flush();
                eprintln!("{}", arguments[0]);
                return Ok(Value::Void);
            }
            "Process.Exit" => {
                return match arguments.as_slice() {
                    [Value::Int(code)] => i32::try_from(*code).map(Halt::Exit).map_or_else(
                        |_| Err(runtime("`Process.Exit` code is out of range").into()),
                        Err,
                    ),
                    _ => Err(runtime("`Process.Exit` expects one int").into()),
                };
            }
            "Env.Args" => {
                if !arguments.is_empty() {
                    return Err(runtime("`Env.Args` expects no arguments").into());
                }
                return Ok(Value::List(
                    self.args.iter().cloned().map(Value::String).collect(),
                ));
            }
            _ => {}
        }
        if let Some(result) = builtins::call(name, &arguments) {
            return result.map_err(Halt::from);
        }

        let (target_module, function_name) = self.resolve(module, name)?;
        let modules: &'a Modules = self.modules;
        let function = modules.programs[&target_module]
            .functions
            .iter()
            .find(|f| f.name == function_name)
            .expect("resolve only returns existing functions");
        if function.parameters.len() != arguments.len() {
            return Err(runtime(format!(
                "`{name}` expects {} argument(s), received {}",
                function.parameters.len(),
                arguments.len()
            ))
            .into());
        }
        let mut scopes = Scopes::new();
        if let Some(globals) = self.globals.get(&target_module) {
            for (name, value) in globals {
                scopes.define(name, value.clone(), false)?;
            }
        }
        for (parameter, value) in function.parameters.iter().zip(arguments) {
            ensure_type(&value, &parameter.ty)?;
            scopes.define(&parameter.name, value, parameter.mutable)?;
        }
        let result = match self.execute_block(&target_module, &function.body, &mut scopes)? {
            Flow::Return(value) => value,
            Flow::Normal => Value::Void,
            Flow::Break | Flow::Skip => {
                return Err(runtime("loop control escaped its loop").into());
            }
        };
        ensure_type(&result, &function.return_type)?;
        Ok(result)
    }

    /// `Name` is a function of the calling module; `Alias.Name` is a `pub` function of a
    /// module the calling module imported with `@use`, named by its last segment.
    fn resolve(&self, module: &str, name: &str) -> Result<(String, String), Diagnostic> {
        let program = &self.modules.programs[module];
        let Some((alias, function)) = name.rsplit_once('.') else {
            return if program.functions.iter().any(|f| f.name == name) {
                Ok((module.to_owned(), name.to_owned()))
            } else {
                Err(runtime(format!("unknown function `{name}`")))
            };
        };
        let imported = program
            .uses
            .iter()
            .find(|used| used.rsplit('.').next() == Some(alias))
            .ok_or_else(|| {
                runtime(format!(
                    "unknown function `{name}`: no `@use` names a module `{alias}`"
                ))
            })?;
        let target = self.modules.programs[imported]
            .functions
            .iter()
            .find(|f| f.name == function)
            .ok_or_else(|| runtime(format!("module `{imported}` has no function `{function}`")))?;
        if !target.public {
            return Err(runtime(format!(
                "`{function}` is private to module `{imported}`; declare it `pub func` to call it from `{module}`"
            )));
        }
        Ok((imported.clone(), function.to_owned()))
    }

    fn execute_block(
        &mut self,
        module: &str,
        statements: &[Statement],
        scopes: &mut Scopes,
    ) -> Result<Flow, Halt> {
        scopes.frames.push(HashMap::new());
        let flow = self.execute_statements(module, statements, scopes);
        scopes.frames.pop();
        flow
    }

    fn execute_statements(
        &mut self,
        module: &str,
        statements: &[Statement],
        scopes: &mut Scopes,
    ) -> Result<Flow, Halt> {
        for statement in statements {
            match statement {
                Statement::Bind {
                    name,
                    mutable,
                    ty,
                    value,
                } => {
                    let value = self.evaluate(module, value, scopes)?;
                    if let Some(ty) = ty {
                        ensure_type(&value, ty)?;
                    }
                    scopes.define(name, value, *mutable)?;
                }
                Statement::Assign { name, value } => {
                    let value = self.evaluate(module, value, scopes)?;
                    scopes.assign(name, value)?;
                }
                Statement::Return(expression) => {
                    return Ok(Flow::Return(match expression {
                        Some(expr) => self.evaluate(module, expr, scopes)?,
                        None => Value::Void,
                    }));
                }
                Statement::If {
                    condition,
                    then_body,
                    else_body,
                } => {
                    let branch = match self.evaluate(module, condition, scopes)? {
                        Value::Bool(true) => then_body,
                        Value::Bool(false) => else_body,
                        _ => return Err(runtime("if condition must be `bool`").into()),
                    };
                    let flow = self.execute_block(module, branch, scopes)?;
                    if !matches!(flow, Flow::Normal) {
                        return Ok(flow);
                    }
                }
                Statement::For {
                    name,
                    iterable,
                    body,
                } => match iterable {
                    Iterable::Range { start, end, step } => {
                        let (Value::Int(start), Value::Int(end)) = (
                            self.evaluate(module, start, scopes)?,
                            self.evaluate(module, end, scopes)?,
                        ) else {
                            return Err(runtime("a `for` range needs int bounds").into());
                        };
                        let magnitude = match step {
                            Some(step) => match self.evaluate(module, step, scopes)? {
                                Value::Int(value) if value > 0 => value,
                                Value::Int(_) => {
                                    return Err(runtime(
                                        "a range `by` step must be greater than zero",
                                    )
                                    .into());
                                }
                                _ => return Err(runtime("a range `by` step must be an int").into()),
                            },
                            None => 1,
                        };
                        let step = if start <= end { magnitude } else { -magnitude };
                        let mut current = start;
                        loop {
                            if (step > 0 && current > end) || (step < 0 && current < end) {
                                break;
                            }
                            match self.execute_loop_iteration(
                                module,
                                name,
                                Value::Int(current),
                                body,
                                scopes,
                            )? {
                                Flow::Return(value) => return Ok(Flow::Return(value)),
                                Flow::Break => break,
                                Flow::Normal | Flow::Skip => {}
                            }
                            if current == end {
                                break;
                            }
                            let Some(next) = current.checked_add(step) else {
                                break;
                            };
                            current = next;
                        }
                    }
                    Iterable::List(list) => {
                        let Value::List(items) = self.evaluate(module, list, scopes)? else {
                            return Err(runtime("`for ... in` needs a list or a range").into());
                        };
                        for item in items {
                            match self.execute_loop_iteration(module, name, item, body, scopes)? {
                                Flow::Return(value) => return Ok(Flow::Return(value)),
                                Flow::Break => break,
                                Flow::Normal | Flow::Skip => {}
                            }
                        }
                    }
                },
                Statement::While { condition, body } => loop {
                    match self.evaluate(module, condition, scopes)? {
                        Value::Bool(true) => {}
                        Value::Bool(false) => break,
                        _ => return Err(runtime("while condition must be `bool`").into()),
                    }
                    match self.execute_block(module, body, scopes)? {
                        Flow::Return(value) => return Ok(Flow::Return(value)),
                        Flow::Break => break,
                        Flow::Normal | Flow::Skip => {}
                    }
                },
                Statement::Loop { body } => loop {
                    match self.execute_block(module, body, scopes)? {
                        Flow::Return(value) => return Ok(Flow::Return(value)),
                        Flow::Break => break,
                        Flow::Normal | Flow::Skip => {}
                    }
                },
                Statement::Break => return Ok(Flow::Break),
                Statement::Skip => return Ok(Flow::Skip),
                Statement::Ask {
                    binding,
                    value,
                    else_body,
                } => match self.evaluate(module, value, scopes) {
                    Ok(value) => {
                        if let Some(binding) = binding {
                            if let Some(ty) = &binding.ty {
                                ensure_type(&value, ty)?;
                            }
                            scopes.define(&binding.name, value, binding.mutable)?;
                        }
                    }
                    Err(Halt::Error(failure)) if failure.recoverable => {
                        scopes.frames.push(HashMap::new());
                        let flow = scopes
                            .define("error", Value::String(failure.message), false)
                            .map_err(Halt::from)
                            .and_then(|()| self.execute_block(module, else_body, scopes));
                        scopes.frames.pop();
                        match flow? {
                            Flow::Return(value) => return Ok(Flow::Return(value)),
                            Flow::Normal if binding.is_some() => {
                                return Err(runtime(
                                    "the `else` block of `let ... = ask` must `return` or call `Process.Exit`",
                                )
                                .into());
                            }
                            Flow::Normal => {}
                            Flow::Break => return Ok(Flow::Break),
                            Flow::Skip => return Ok(Flow::Skip),
                        }
                    }
                    Err(halt) => return Err(halt),
                },
                Statement::Expression(expression) => {
                    self.evaluate(module, expression, scopes)?;
                }
            }
        }
        Ok(Flow::Normal)
    }

    fn execute_loop_iteration(
        &mut self,
        module: &str,
        name: &str,
        item: Value,
        body: &[Statement],
        scopes: &mut Scopes,
    ) -> Result<Flow, Halt> {
        scopes.frames.push(HashMap::new());
        let flow = scopes
            .define(name, item, false)
            .map_err(Halt::from)
            .and_then(|()| self.execute_block(module, body, scopes));
        scopes.frames.pop();
        flow
    }

    fn evaluate(
        &mut self,
        module: &str,
        expression: &Expression,
        scopes: &Scopes,
    ) -> Result<Value, Halt> {
        match expression {
            Expression::Integer(v) => Ok(Value::Int(*v)),
            Expression::Float(v) => Ok(Value::Float(*v)),
            Expression::Bool(v) => Ok(Value::Bool(*v)),
            Expression::String(v) => Ok(Value::String(interpolate(v, scopes)?)),
            Expression::List(items) => Ok(Value::List(
                items
                    .iter()
                    .map(|item| self.evaluate(module, item, scopes))
                    .collect::<Result<_, _>>()?,
            )),
            Expression::Variable(name) => resolve_value(scopes, name).map_err(Halt::from),
            Expression::Pack { name, fields } => {
                let definition = self.modules.programs[module]
                    .packs
                    .iter()
                    .find(|pack| pack.name == *name)
                    .ok_or_else(|| runtime(format!("unknown pack `{name}`")))?;
                let mut values = BTreeMap::new();
                for (field, expression) in fields {
                    let Some(expected) = definition.fields.iter().find(|item| item.name == *field)
                    else {
                        return Err(
                            runtime(format!("unknown field `{field}` for pack `{name}`")).into(),
                        );
                    };
                    let value = self.evaluate(module, expression, scopes)?;
                    ensure_type(&value, &expected.ty)?;
                    values.insert(field.clone(), value);
                }
                if let Some(missing) = definition
                    .fields
                    .iter()
                    .find(|field| !values.contains_key(&field.name))
                {
                    return Err(runtime(format!(
                        "missing field `{}` for pack `{name}`",
                        missing.name
                    ))
                    .into());
                }
                Ok(Value::Pack {
                    name: name.clone(),
                    fields: values,
                })
            }
            Expression::Unary { operator, operand } => {
                let value = self.evaluate(module, operand, scopes)?;
                match (operator, value) {
                    (UnaryOperator::Negate, Value::Int(v)) => v
                        .checked_neg()
                        .map(Value::Int)
                        .ok_or_else(|| runtime("integer overflow").into()),
                    (UnaryOperator::Negate, Value::Float(v)) => Ok(Value::Float(-v)),
                    (UnaryOperator::Not, Value::Bool(v)) => Ok(Value::Bool(!v)),
                    _ => Err(runtime("invalid unary operation").into()),
                }
            }
            Expression::Binary {
                left,
                operator,
                right,
            } => {
                let left = self.evaluate(module, left, scopes)?;
                let right = self.evaluate(module, right, scopes)?;
                Ok(binary(left, *operator, right)?)
            }
            Expression::Logical {
                left,
                operator,
                right,
            } => {
                let Value::Bool(left) = self.evaluate(module, left, scopes)? else {
                    return Err(runtime("`&&` and `||` need bool operands").into());
                };
                if matches!(
                    (operator, left),
                    (LogicalOperator::And, false) | (LogicalOperator::Or, true)
                ) {
                    return Ok(Value::Bool(left));
                }
                match self.evaluate(module, right, scopes)? {
                    Value::Bool(right) => Ok(Value::Bool(right)),
                    _ => Err(runtime("`&&` and `||` need bool operands").into()),
                }
            }
            Expression::Index { target, index } => {
                match (
                    self.evaluate(module, target, scopes)?,
                    self.evaluate(module, index, scopes)?,
                ) {
                    (Value::List(items), Value::Int(index)) => usize::try_from(index)
                        .ok()
                        .and_then(|index| items.get(index).cloned())
                        .ok_or_else(|| {
                            runtime(format!(
                                "list index {index} is out of range for a list of {} item(s)",
                                items.len()
                            ))
                            .into()
                        }),
                    _ => Err(runtime("indexing needs a list and an int").into()),
                }
            }
            Expression::Call { name, arguments } => {
                let values = arguments
                    .iter()
                    .map(|arg| self.evaluate(module, arg, scopes))
                    .collect::<Result<Vec<_>, _>>()?;
                self.call(module, name, values)
            }
        }
    }
}

fn interpolate(text: &str, scopes: &Scopes) -> Result<String, Diagnostic> {
    let mut output = String::new();
    let mut rest = text;
    while let Some(start) = rest.find('{') {
        output.push_str(&rest[..start]);
        let tail = &rest[start + 1..];
        let end = tail
            .find('}')
            .ok_or_else(|| runtime("unclosed interpolation in string"))?;
        let name = &tail[..end];
        let value = resolve_value(scopes, name)?;
        output.push_str(&value.to_string());
        rest = &tail[end + 1..];
    }
    output.push_str(rest);
    Ok(output)
}

fn binary(left: Value, operator: BinaryOperator, right: Value) -> Result<Value, Diagnostic> {
    use BinaryOperator::*;
    match (left, operator, right) {
        (Value::Int(a), Add, Value::Int(b)) => checked_integer(a.checked_add(b)),
        (Value::Float(a), Add, Value::Float(b)) => checked_float(a + b),
        (Value::String(a), Add, Value::String(b)) => Ok(Value::String(a + &b)),
        (Value::Int(a), Subtract, Value::Int(b)) => checked_integer(a.checked_sub(b)),
        (Value::Float(a), Subtract, Value::Float(b)) => checked_float(a - b),
        (Value::Int(a), Multiply, Value::Int(b)) => checked_integer(a.checked_mul(b)),
        (Value::Float(a), Multiply, Value::Float(b)) => checked_float(a * b),
        (Value::Int(_), Divide, Value::Int(0)) => Err(runtime("division by zero")),
        (Value::Int(a), Divide, Value::Int(b)) => checked_integer(a.checked_div(b)),
        (Value::Float(_), Divide, Value::Float(0.0)) => Err(runtime("division by zero")),
        (Value::Float(a), Divide, Value::Float(b)) => checked_float(a / b),
        (Value::Int(_), Remainder, Value::Int(0)) => Err(runtime("remainder by zero")),
        (Value::Int(a), Remainder, Value::Int(b)) => checked_integer(a.checked_rem(b)),
        (Value::Float(_), Remainder, Value::Float(0.0)) => Err(runtime("remainder by zero")),
        (Value::Float(a), Remainder, Value::Float(b)) => checked_float(a % b),
        (a, Equal, b) => Ok(Value::Bool(a == b)),
        (a, NotEqual, b) => Ok(Value::Bool(a != b)),
        (Value::Int(a), Less, Value::Int(b)) => Ok(Value::Bool(a < b)),
        (Value::Float(a), Less, Value::Float(b)) => Ok(Value::Bool(a < b)),
        (Value::Int(a), LessEqual, Value::Int(b)) => Ok(Value::Bool(a <= b)),
        (Value::Float(a), LessEqual, Value::Float(b)) => Ok(Value::Bool(a <= b)),
        (Value::Int(a), Greater, Value::Int(b)) => Ok(Value::Bool(a > b)),
        (Value::Float(a), Greater, Value::Float(b)) => Ok(Value::Bool(a > b)),
        (Value::Int(a), GreaterEqual, Value::Int(b)) => Ok(Value::Bool(a >= b)),
        (Value::Float(a), GreaterEqual, Value::Float(b)) => Ok(Value::Bool(a >= b)),
        _ => Err(runtime("invalid binary operation")),
    }
}

fn checked_integer(value: Option<i64>) -> Result<Value, Diagnostic> {
    value
        .map(Value::Int)
        .ok_or_else(|| runtime("integer overflow"))
}

fn checked_float(value: f64) -> Result<Value, Diagnostic> {
    if value.is_finite() {
        Ok(Value::Float(value))
    } else {
        Err(runtime("float result is not finite"))
    }
}

fn ensure_type(value: &Value, ty: &Type) -> Result<(), Diagnostic> {
    if matches_type(value, ty) {
        Ok(())
    } else {
        Err(runtime("value does not match declared type"))
    }
}

fn matches_type(value: &Value, ty: &Type) -> bool {
    match (value, ty) {
        (Value::Int(_), Type::Int)
        | (Value::Float(_), Type::Float)
        | (Value::Bool(_), Type::Bool)
        | (Value::String(_), Type::String)
        | (Value::Void, Type::Void) => true,
        (Value::List(items), Type::List(element)) => {
            items.iter().all(|item| matches_type(item, element))
        }
        (Value::Pack { name, .. }, Type::Named(expected)) => name == expected,
        _ => false,
    }
}

/// Resolves a local binding followed by zero or more pack fields.
fn resolve_value(scopes: &Scopes, path: &str) -> Result<Value, Diagnostic> {
    let mut segments = path.split('.');
    let root = segments.next().unwrap_or(path);
    let mut value = scopes
        .get(root)
        .map(|binding| binding.value.clone())
        .ok_or_else(|| runtime(format!("unknown variable `{root}`")))?;
    for field in segments {
        value = match value {
            Value::Pack { name, fields } => fields
                .get(field)
                .cloned()
                .ok_or_else(|| runtime(format!("pack `{name}` has no field `{field}`")))?,
            _ => {
                return Err(runtime(format!(
                    "cannot access field `{field}` on a non-pack value"
                )));
            }
        };
    }
    Ok(value)
}

fn runtime(message: impl Into<String>) -> Diagnostic {
    Diagnostic::new(message, 0, 0)
}
