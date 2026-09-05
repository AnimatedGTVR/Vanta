pub mod ast;
pub mod diagnostic;
pub mod interpreter;
pub mod lexer;
pub mod parser;
pub mod token;

use diagnostic::Diagnostic;

pub fn run(source: &str) -> Result<String, Diagnostic> {
    let tokens = lexer::lex(source)?;
    let program = parser::parse(tokens)?;
    interpreter::interpret(&program)
}

#[cfg(test)]
mod tests {
    use super::run;

    #[test]
    fn runs_functions_arithmetic_and_interpolation() {
        let source = r#"
            module Main;
            func Add(let a::int, let b::int)::int { return a + b; }
            func Start()::void {
                let answer::int = Add(20, 22);
                emit("Answer: {answer}");
            }
        "#;
        assert_eq!(run(source).unwrap(), "Answer: 42\n");
    }

    #[test]
    fn supports_mutability_and_conditionals() {
        let source = r#"
            module Main;
            func Start()::void {
                mut score = 2;
                score = score * 5;
                if score >= 10 { emit("win"); } else { emit("lose"); }
            }
        "#;
        assert_eq!(run(source).unwrap(), "win\n");
    }

    #[test]
    fn rejects_assignment_to_let() {
        let source = "module Main; func Start()::void { let x = 1; x = 2; }";
        assert!(run(source).unwrap_err().message.contains("immutable"));
    }

    #[test]
    fn rejects_out_of_range_integer_literals() {
        let source = "module Main; func Start()::void { emit(9223372036854775808); }";
        assert!(run(source).unwrap_err().message.contains("out of range"));
    }

    #[test]
    fn reports_integer_overflow() {
        let source = "module Main; func Start()::void { emit(9223372036854775807 + 1); }";
        assert!(run(source).unwrap_err().message.contains("overflow"));
    }

    #[test]
    fn limits_recursive_calls() {
        let source = r#"
            module Main;
            func Loop()::void { Loop(); }
            func Start()::void { Loop(); }
        "#;
        assert!(
            run(source)
                .unwrap_err()
                .message
                .contains("maximum call depth")
        );
    }
}
