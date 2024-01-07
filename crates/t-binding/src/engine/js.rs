use std::collections::HashMap;

use crate::{api, ScriptEngine};
use quick_js::Context;

pub struct JSEngine {
    cx: quick_js::Context,
}

impl ScriptEngine for JSEngine {
    fn run(&mut self, content: &str) {
        self.run(content).unwrap();
    }
}

impl JSEngine {
    fn _new() -> Self {
        let e = Self {
            cx: Context::new().unwrap(),
        };

        e.cx.add_callback("print", api::print).unwrap();
        e.cx.add_callback(
            "assert_script_run_ssh_seperate",
            api::assert_script_run_ssh_seperate,
        )
        .unwrap();

        e.cx.add_callback(
            "assert_script_run_ssh_global",
            api::assert_script_run_ssh_global,
        )
        .unwrap();

        e.cx.add_callback(
            "assert_script_run_serial_global",
            move |cmd: String, timeout: i32| -> String {
                let res = api::assert_script_run_serial_global(cmd, timeout);
                res.1
            },
        )
        .unwrap();

        e
    }

    pub fn new() -> Box<dyn ScriptEngine> {
        let e = Self::_new();
        Box::new(e)
    }

    pub fn run(&mut self, content: &str) -> Result<(), String> {
        self.cx.eval(content).map_err(|e| e.to_string())?;
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use quick_js::{Context, JsValue};

    use crate::JSEngine;

    #[test]
    #[should_panic]
    fn test_engine() {
        JSEngine::_new()
            .run(
                r#"
                print("1");
                assert_script_run("whoami", 600);
                assert_script_run("sleep 10", 12000);
                print("2");
            "#,
            )
            .unwrap();
    }

    #[test]
    fn test_quickjs_basic() {
        use quick_js::{Context, JsValue};

        let context = Context::new().unwrap();

        // Eval.
        let value = context.eval("1 + 2").unwrap();
        assert_eq!(value, JsValue::Int(3));

        let value = context
            .eval_as::<String>(
                "() => {
                var x = 100 + 250;
                return x.toString();
            }()",
            )
            .unwrap();
        assert_eq!(&value, "350");

        // Callbacks.
        context
            .add_callback("myCallback", |a: i32, b: i32| a + b)
            .unwrap();

        let hello_str = "hello";
        let hello = move || {
            println!("{}", hello_str);
        };
        context
            .add_callback("assert_screen", move |needle_tag: String, timeout: i32| {
                hello();
                timeout * 2
            })
            .unwrap();

        let value = context
            .eval(
                r#"
                const wait = async () => {
                };
                const main = async () => {
                    await wait();
                    return 30;
                };
                main();
            "#,
            )
            .unwrap();
        assert_eq!(value, JsValue::Int(30));
    }

    #[test]
    fn test_quickjs_console() {
        let ctx = Context::builder()
            .console(|level, args| {
                eprintln!("{}: {:?}", level, args);
            })
            .build()
            .unwrap();
        ctx.eval(r#"console.log(1, "2", {c: 3})"#).unwrap();
    }

    #[test]
    fn test_quickjs_json() {
        let ctx = Context::builder()
            .console(|level, args| {
                eprintln!("{}: {:?}", level, args);
            })
            .build()
            .unwrap();
        ctx.eval(r#"console.log(1, "2", JSON.stringify({c: 3}) )"#)
            .unwrap();
    }

    #[test]
    #[should_panic]
    fn test_quickjs_module() {
        let ctx = Context::new().unwrap();

        ctx.add_callback("print", move |cmd: String| {
            println!("{cmd}");
            JsValue::Null
        })
        .unwrap();

        // Evaluate lib.js
        ctx.eval(
            r#"
            export function add(a, b) {
                return a + b;
            }
        "#,
        )
        .unwrap();

        // Evaluate main.js
        ctx.eval(
            r#"
            import { add } from './lib.js';

            print(add(3, 4));
        "#,
        )
        .unwrap();
    }
}
