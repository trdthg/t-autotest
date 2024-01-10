use std::sync::{Arc, Mutex};

use rquickjs::Function;
use rquickjs::{Context, Runtime};
use tracing::{info, warn};

use crate::{api, ScriptEngine};

pub struct JSEngine {
    runtime: rquickjs::Runtime,
    context: rquickjs::Context,
    outputs: Arc<Mutex<Vec<(String, String)>>>,
}

impl ScriptEngine for JSEngine {
    fn run(&mut self, content: &str) {
        self.run(content).unwrap();
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

                ctx.globals()
                    .set(
                        "assert_script_run_serial_global",
                        Function::new(ctx.clone(), move |cmd: String, timeout: i32| -> String {
                            let res = api::assert_script_run_serial_global(cmd, timeout);
                            res.1
                        }),
                    )
                    .unwrap();

                ctx.globals()
                    .set(
                        "print",
                        Function::new(ctx.clone(), move |msg: String| {
                            api::print(msg);
                        }),
                    )
                    .unwrap();
                ctx.globals()
                    .set(
                        "assert_script_run_ssh_seperate",
                        Function::new(ctx.clone(), api::assert_script_run_ssh_seperate),
                    )
                    .unwrap();

                ctx.globals()
                    .set(
                        "assert_script_run_ssh_global",
                        Function::new(ctx.clone(), api::assert_script_run_ssh_global),
                    )
                    .unwrap();

                ctx.globals()
                    .set(
                        "assert_screen",
                        Function::new(
                            ctx.clone(),
                            move |tag: String, timeout: i32| -> rquickjs::Result<()> {
                                let res = match api::assert_screen(tag.clone(), timeout) {
                                    Ok(res) => {
                                        info!(api = "assert_screen", result = res, tag = tag,);
                                        res
                                    }
                                    Err(_) => {
                                        warn!(api = "assert_screen", result = "err",);
                                        false
                                    }
                                };
                                if !res {
                                    Err(rquickjs::Error::Exception)
                                } else {
                                    Ok(())
                                }
                            },
                        ),
                    )
                    .unwrap();

                ctx.eval(
                    r#"var console = Object.freeze({
                            log(data){__rust_log__("log",JSON.stringify(data))},
                            info(data){__rust_log__("info",JSON.stringify(data))},
                            error(data){__rust_log__("error",JSON.stringify(data))},
                            debug(data){__rust_log__("debug",JSON.stringify(data))},
                        });"#,
                )
                .map_err(|_| ())?;

                ctx.globals()
                    .set("mouse_click", Function::new(ctx.clone(), api::mouse_click))
                    .unwrap();

                ctx.globals()
                    .set("mouse_move", Function::new(ctx.clone(), api::mouse_move))
                    .unwrap();

                ctx.globals()
                    .set("mouse_hide", Function::new(ctx.clone(), api::mouse_hide))
                    .unwrap();

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
    {script}

    if (typeof prehook === 'function') {{
        prehook();
    }}

    // run user defined run
    let res = run() || ''

    if (typeof afterhook === 'function') {{
        afterhook();
    }}

    // return
    JSON.stringify({{
        code: 0,
        msg: "success",
        data: JSON.stringify(res),
    }})
}} catch(err) {{
    // return
    JSON.stringify({{
        code: 1,
        msg: err.toString(),
        data: "",
    }})
}}
"#
        );

        self.context
            .with(|ctx| match ctx.eval::<String, &str>(code.as_str()) {
                Ok(result) => {
                    println!("result: [{}]", result);
                }
                Err(e) => {
                    println!("e: [{}]", e.to_string());
                    println!("code: [{}]", code);
                }
            });

        let out = self.outputs.lock().unwrap().to_vec();

        Ok(())
    }
}

#[cfg(test)]
mod test {

    use super::JSEngine;
    use rquickjs::{function::Args, Context, Runtime};

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
