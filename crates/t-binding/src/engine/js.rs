use crate::{api, ScriptEngine};
use rquickjs::Function;
use rquickjs::{Context, Runtime};
use serde::{Deserialize, Serialize};
use tracing::{error, info, Level};

pub struct JSEngine {
    _runtime: rquickjs::Runtime,
    context: rquickjs::Context,
}

impl ScriptEngine for JSEngine {
    fn run(&mut self, content: &str) {
        self.run(content).unwrap();
    }
}

impl Default for JSEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl JSEngine {
    pub fn new() -> Self {
        let runtime = Runtime::new().unwrap();
        let context = Context::full(&runtime).unwrap();

        context
            .with(|ctx| -> Result<(), ()> {
                // general
                ctx.globals()
                    .set(
                        "print",
                        Function::new(ctx.clone(), move |msg: String| {
                            api::print(Level::INFO, msg);
                        }),
                    )
                    .unwrap();
                ctx.globals()
                    .set("sleep", Function::new(ctx.clone(), api::sleep))
                    .unwrap();

                ctx.globals()
                    .set("get_env", Function::new(ctx.clone(), api::get_env))
                    .unwrap();
                ctx.globals()
                    .set(
                        "__rust_log__",
                        Function::new(ctx.clone(), move |level: String, msg: String| {
                            match level.as_str() {
                                "log" | "info" => api::print(Level::INFO, msg),
                                "error" => api::print(Level::ERROR, msg),
                                "debug" => api::print(Level::DEBUG, msg),
                                _ => {}
                            }
                        }),
                    )
                    .unwrap();
                ctx.eval(
                    r#"
                        var console = Object.freeze({
                            log(data){__rust_log__("log",JSON.stringify(data))},
                            info(data){__rust_log__("info",JSON.stringify(data))},
                            error(data){__rust_log__("error",JSON.stringify(data))},
                            debug(data){__rust_log__("debug",JSON.stringify(data))},
                        });"#,
                )
                .map_err(|_| ())?;

                // ssh
                ctx.globals()
                    .set(
                        "ssh_assert_script_run_global",
                        Function::new(ctx.clone(), api::ssh_assert_script_run_global),
                    )
                    .unwrap();
                ctx.globals()
                    .set(
                        "ssh_assert_script_run_seperate",
                        Function::new(ctx.clone(), api::ssh_assert_script_run_seperate),
                    )
                    .unwrap();
                ctx.globals()
                    .set(
                        "ssh_write_string",
                        Function::new(ctx.clone(), move |s: String| api::ssh_write_string(s)),
                    )
                    .unwrap();

                // serial
                ctx.globals()
                    .set(
                        "serial_assert_script_run_global",
                        Function::new(
                            ctx.clone(),
                            move |cmd: String, timeout: i32| -> rquickjs::Result<String> {
                                let res = api::serial_assert_script_run_global(cmd, timeout);
                                res.ok_or(rquickjs::Error::Exception)
                            },
                        ),
                    )
                    .unwrap();
                ctx.globals()
                    .set(
                        "serial_script_run_global",
                        Function::new(ctx.clone(), move |cmd: String, timeout: i32| -> String {
                            api::serial_script_run_global(cmd, timeout).unwrap()
                        }),
                    )
                    .unwrap();
                ctx.globals()
                    .set(
                        "serial_script_run_global",
                        Function::new(ctx.clone(), move |cmd: String, timeout: i32| -> String {
                            api::serial_script_run_global(cmd, timeout).unwrap()
                        }),
                    )
                    .unwrap();
                ctx.globals()
                    .set(
                        "serial_write_string",
                        Function::new(ctx.clone(), move |s: String| api::serial_write_string(s)),
                    )
                    .unwrap();

                // vnc
                ctx.globals()
                    .set(
                        "assert_screen",
                        Function::new(
                            ctx.clone(),
                            move |tag: String, timeout: i32| -> rquickjs::Result<()> {
                                let res = api::vnc_check_screen(tag.clone(), timeout);
                                if !res {
                                    Err(rquickjs::Error::Exception)
                                } else {
                                    Ok(())
                                }
                            },
                        ),
                    )
                    .unwrap();
                ctx.globals()
                    .set(
                        "check_screen",
                        Function::new(ctx.clone(), move |tag: String, timeout: i32| -> bool {
                            api::vnc_check_screen(tag.clone(), timeout)
                        }),
                    )
                    .unwrap();
                ctx.globals()
                    .set(
                        "mouse_click",
                        Function::new(ctx.clone(), api::vnc_mouse_click),
                    )
                    .unwrap();

                ctx.globals()
                    .set(
                        "mouse_move",
                        Function::new(ctx.clone(), api::vnc_mouse_move),
                    )
                    .unwrap();

                ctx.globals()
                    .set(
                        "mouse_hide",
                        Function::new(ctx.clone(), api::vnc_mouse_hide),
                    )
                    .unwrap();

                Ok(())
            })
            .unwrap();

        Self {
            _runtime: runtime,
            context,
        }
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
        info!(msg = "script code", code = ?code);
        self.context
            .with(|ctx| match ctx.eval::<String, &str>(code.as_str()) {
                Ok(result) => {
                    let result: Response =
                        serde_json::from_str(&result).expect("js script wrong return type");
                    if result.code != 0 {
                        error!(msg = "js script run failed", reason = result.msg);
                    }
                    info!(msg = "script run success", result = ?result);
                }
                Err(e) => {
                    error!(
                        msg = "script run failed, assert_xxx throw exception",
                        reason = e.to_string(),
                    );
                }
            });

        Ok(())
    }
}

#[derive(Serialize, Deserialize, Debug)]
struct Response {
    code: i32,
    msg: String,
    data: String,
}

#[cfg(test)]
mod test {

    use super::JSEngine;
    use rquickjs::{function::Args, Context, Runtime};

    fn get_context() -> rquickjs::Context {
        let runtime = Runtime::new().unwrap();

        Context::full(&runtime).unwrap()
    }

    #[test]
    fn test_engine() {
        JSEngine::new()
            .run(
                r#"
        print("1");
        assert_script_run("whoami", 600);
        print("2");
    "#,
            )
            .unwrap();
    }

    #[test]
    fn test_quickjs_basic() {
        get_context().with(|ctx| {
            let func_add =
                rquickjs::Function::new(ctx.clone(), move |a: i32, b: i32| -> i32 { a + b })
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
