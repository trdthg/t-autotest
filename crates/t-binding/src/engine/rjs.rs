use std::sync::{Arc, Mutex};

use rquickjs::Function;
use rquickjs::{Context, Runtime};

use crate::{api, ScriptEngine};

pub struct JSEngine {
    runtime: rquickjs::Runtime,
    context: rquickjs::Context,
    outputs: Arc<Mutex<Vec<(String, String)>>>,
}

impl ScriptEngine for JSEngine {
    fn run(&mut self, content: &str) {
        // todo!()
    }
}

impl JSEngine {
    pub fn new() -> Box<dyn ScriptEngine> {
        let runtime = Runtime::new().unwrap();
        let context = Context::full(&runtime).unwrap();
        let outputs = Arc::new(Mutex::new(vec![]));

        let outputs_clone = outputs.clone();
        context
            .with(|ctx| -> Result<(), ()> {
                let func_log = Function::new(ctx.clone(), move |level: String, msg: String| {
                    let mut out = outputs_clone.lock().unwrap();
                    out.push((level, msg));
                })
                .unwrap();
                ctx.globals()
                    .set("__rust_log__", func_log)
                    .map_err(|_| ())?;

                let func_assert_script_run_serial_global =
                    Function::new(ctx.clone(), move |cmd: String, timeout: i32| -> String {
                        api::assert_script_run_serial_global(cmd, timeout).1
                    })
                    .unwrap();
                ctx.globals()
                    .set(
                        "assert_script_run_serial_global",
                        func_assert_script_run_serial_global,
                    )
                    .map_err(|_| ())?;

                ctx.eval(
                    r#"var console = Object.freeze({
                            log(data){__rust_log__("log",JSON.stringify(data))},
                            info(data){__rust_log__("info",JSON.stringify(data))},
                            error(data){__rust_log__("error",JSON.stringify(data))},
                            debug(data){__rust_log__("debug",JSON.stringify(data))},
                        });"#,
                )
                .map_err(|_| ())?;

                Ok(())
            })
            .unwrap();

        let e = Self {
            runtime,
            context,
            outputs,
        };

        Box::new(e)
    }

    pub fn run(&mut self, script: &str) -> Result<(), String> {
        let code = format!(
            r#"try{{
                // load user script
                {script};

                if (typeof myFunction === 'function') {{
                    prehook();
                }}

                let res = main() || ''

                if (typeof myFunction === 'function') {{
                    afterhook();
                }}

                // return
                {{
                    code: 0,
                    msg: "success",
                    data: JSON.stringify(res),
                }}
            }} catch(err) {{
                // return
                {{
                    code: 1,
                    msg: err.toString(),
                    data: "",
                }}
            }}"#
        );

        self.context.with(|ctx| {
            let result: String = ctx.eval(code.as_str()).map_err(|e| ()).unwrap();
            if result.starts_with("__error_flag__") {
                // anyhow::bail!(result[15..].to_owned());
            }
            if result == "\"\"" {
                // anyhow::bail!("main function should return object");
            }
        });

        let mut out = self.outputs.lock().unwrap().to_vec();

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use std::{fs, path::Path};

    use super::JSEngine;
    use rquickjs::{context::EvalOptions, function::Args, Context, Runtime};

    fn get_context() -> rquickjs::Context {
        let runtime = Runtime::new().unwrap();
        let context = Context::full(&runtime).unwrap();
        context
    }

    #[test]
    fn test_engine() {
        JSEngine::new().run(
            r#"
        print("1");
        assert_script_run("whoami", 600);
        print("2");
    "#,
        );
    }

    #[test]
    fn test_quickjs_basic() {
        get_context().with(|ctx| {
            let func_add = rquickjs::Function::new(ctx.clone(), move |a: i32, b: i32| -> i32 {
                return a + b;
            })
            .unwrap();
            ctx.globals().set("add", func_add).map_err(|_| ()).unwrap();

            let value = ctx
                .eval::<i32, &str>(
                    r#"
            const add_ = (a, b) => {
                return a + b;
            }
            add(add_(1, 2), 3)
            "#,
                )
                .unwrap();
            assert_eq!(value, 6);
        });
    }

    #[test]
    // #[should_panic]
    fn test_quickjs_module() {
        get_context().with(|ctx| {
            println!("{}", std::env::current_dir().unwrap().display());

            let func_log = rquickjs::Function::new(ctx.clone(), move |msg: String| {
                println!("{msg}");
            })
            .unwrap();
            ctx.globals().set("print", func_log).unwrap();

            let _module_lib = ctx
                .clone()
                .compile(
                    "./folder1/lib.js",
                    r#"
                        export function add(a, b) {
                            return a + b
                        }
                    "#,
                )
                .unwrap();

            let module_case1 = ctx
                .clone()
                .compile(
                    "./folder1/folder2/case1.js",
                    r#"
                        import { add } from "../lib.js"
                        export function run(c) {
                            return add(1, 2) + c
                        }
                    "#,
                )
                .unwrap();

            let function: rquickjs::Function = module_case1.get("run").unwrap();

            let mut args = Args::new(ctx.clone(), 0);
            args.push_arg(3).unwrap();
            args.push_arg("").unwrap();
            let res = function.call_arg::<i32>(args).unwrap();

            assert_eq!(res, 6);
        });
    }
}
