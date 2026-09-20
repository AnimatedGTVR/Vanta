use std::collections::BTreeMap;

use vanta::diagnostic::Diagnostic;
use vanta::interpreter::{NativeHost, Value};

struct MockEngine;

impl NativeHost for MockEngine {
    fn call(&mut self, name: &str, arguments: &[Value]) -> Option<Result<Value, Diagnostic>> {
        match name {
            "ScriptContext.HasRigidbody2D" => Some(Ok(Value::Bool(true))),
            "ScriptContext.GetMoveInputWASD" => {
                Some(Ok(pack("Vec3", [("x", 3.0), ("y", 0.0), ("z", 4.0)])))
            }
            "ScriptContext.IsSprintDown" => Some(Ok(Value::Bool(true))),
            "ScriptContext.SetRigidbody2DVelocity" => {
                println!("velocity={}", arguments[1]);
                Some(Ok(Value::Void))
            }
            "ScriptContext.SetUILabel" => {
                println!("label={}", arguments[1]);
                Some(Ok(Value::Void))
            }
            _ => None,
        }
    }
}

fn pack<const N: usize>(name: &str, fields: [(&str, f64); N]) -> Value {
    Value::Pack {
        name: name.into(),
        fields: fields
            .into_iter()
            .map(|(name, value)| (name.into(), Value::Float(value)))
            .collect::<BTreeMap<_, _>>(),
    }
}

fn main() {
    let source = include_str!("player_controller.vanta");
    vanta::run_with_host(source, &mut MockEngine).expect("controller should run");
}
