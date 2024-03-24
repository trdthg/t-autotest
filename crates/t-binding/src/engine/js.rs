use std::fs;
use std::path::{Path, PathBuf};

use crate::{api, ApiError, ScriptEngine};
use rquickjs::function::Args;
use rquickjs::Function;
use rquickjs::{Context, Runtime};
use serde::{Deserialize, Serialize};
use tracing::{error, Level};

pub struct JSEngine {
    _runtime: rquickjs::Runtime,
    context: rquickjs::Context,
}

impl ScriptEngine for JSEngine {
    fn run_file(&mut self, content: &str) {
        self.run_file(content).unwrap();
    }
}

impl Default for JSEngine {
    fn default() -> Self {
        Self::new()
    }
}

fn into_jserr(_: ApiError) -> rquickjs::Error {
    rquickjs::Error::Exception
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
                    .set(
                        "get_env",
                        Function::new(
                            ctx.clone(),
                            move |key| -> rquickjs::Result<Option<String>> {
                                api::get_env(key).map_err(into_jserr)
                            },
                        ),
                    )
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

                // general console
                ctx.globals()
                    .set(
                        "assert_script_run_global",
                        Function::new(
                            ctx.clone(),
                            move |cmd: String, timeout: i32| -> rquickjs::Result<String> {
                                let res = api::assert_script_run(cmd, timeout);
                                res.map_err(into_jserr)
                            },
                        ),
                    )
                    .unwrap();
                ctx.globals()
                    .set(
                        "script_run_global",
                        Function::new(
                            ctx.clone(),
                            move |cmd: String, timeout: i32| -> Option<String> {
                                api::script_run(cmd, timeout).map(|v| v.1).ok()
                            },
                        ),
                    )
                    .unwrap();
                ctx.globals()
                    .set(
                        "write_string",
                        Function::new(ctx.clone(), move |s: String| api::write(s).ok()),
                    )
                    .unwrap();

                // ssh
                ctx.globals()
                    .set(
                        "ssh_assert_script_run_global",
                        Function::new(
                            ctx.clone(),
                            move |cmd: String, timeout: i32| -> rquickjs::Result<String> {
                                api::ssh_assert_script_run(cmd, timeout).map_err(into_jserr)
                            },
                        ),
                    )
                    .unwrap();
                ctx.globals()
                    .set(
                        "ssh_script_run_global",
                        Function::new(ctx.clone(), |cmd, timeout| -> rquickjs::Result<String> {
                            api::ssh_script_run(cmd, timeout)
                                .map(|v| v.1)
                                .map_err(into_jserr)
                        }),
                    )
                    .unwrap();
                ctx.globals()
                    .set(
                        "ssh_assert_script_run_seperate",
                        Function::new(
                            ctx.clone(),
                            move |cmd: String, timeout: i32| -> rquickjs::Result<String> {
                                api::ssh_assert_script_run_seperate(cmd, timeout)
                                    .map_err(into_jserr)
                            },
                        ),
                    )
                    .unwrap();
                ctx.globals()
                    .set(
                        "ssh_write_string",
                        Function::new(ctx.clone(), move |s: String| -> rquickjs::Result<()> {
                            api::ssh_write(s).map_err(into_jserr)
                        }),
                    )
                    .unwrap();

                // serial
                ctx.globals()
                    .set(
                        "serial_assert_script_run_global",
                        Function::new(
                            ctx.clone(),
                            move |cmd: String, timeout: i32| -> rquickjs::Result<String> {
                                api::serial_assert_script_run(cmd, timeout).map_err(into_jserr)
                            },
                        ),
                    )
                    .unwrap();
                ctx.globals()
                    .set(
                        "serial_script_run_global",
                        Function::new(
                            ctx.clone(),
                            move |cmd: String, timeout: i32| -> Option<String> {
                                api::serial_script_run(cmd, timeout).map(|v| v.1).ok()
                            },
                        ),
                    )
                    .unwrap();
                ctx.globals()
                    .set(
                        "serial_write_string",
                        Function::new(ctx.clone(), move |s: String| -> rquickjs::Result<()> {
                            api::serial_write(s).map_err(into_jserr)
                        }),
                    )
                    .unwrap();

                // vnc
                ctx.globals()
                    .set(
                        "assert_screen",
                        Function::new(
                            ctx.clone(),
                            move |tag: String, timeout: i32| -> rquickjs::Result<bool> {
                                api::vnc_check_screen(tag.clone(), timeout).map_err(into_jserr)
                            },
                        ),
                    )
                    .unwrap();
                ctx.globals()
                    .set(
                        "check_screen",
                        Function::new(
                            ctx.clone(),
                            move |tag: String, timeout: i32| -> rquickjs::Result<bool> {
                                api::vnc_check_screen(tag.clone(), timeout).map_err(into_jserr)
                            },
                        ),
                    )
                    .unwrap();
                ctx.globals()
                    .set(
                        "mouse_click",
                        Function::new(ctx.clone(), move || -> rquickjs::Result<()> {
                            api::vnc_mouse_click().map_err(into_jserr)
                        }),
                    )
                    .unwrap();

                ctx.globals()
                    .set(
                        "mouse_move",
                        Function::new(ctx.clone(), move |x, y| -> rquickjs::Result<()> {
                            api::vnc_mouse_move(x, y).map_err(into_jserr)
                        }),
                    )
                    .unwrap();

                ctx.globals()
                    .set(
                        "mouse_hide",
                        Function::new(ctx.clone(), move || -> rquickjs::Result<()> {
                            api::vnc_mouse_hide().map_err(into_jserr)
                        }),
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

    pub fn run_file(&mut self, file: &str) -> Result<(), String> {
        let base_folder = Path::new(file).parent().unwrap();
        let filename = Path::new(file).file_name().unwrap().to_str().unwrap();
        let script = fs::read_to_string(file).unwrap();
        let pre_libs = search_path(&script);
        self.context.with(|ctx| {
            for path in pre_libs {
                let mut fullpath = PathBuf::new();
                fullpath.push(base_folder);
                fullpath.push(&path);
                let _ = ctx
                    .clone()
                    .compile(path.as_str(), fs::read_to_string(fullpath).unwrap())
                    .map_err(|e| {
                        format!("lib file: [{}] compile failed: [{}]", path.as_str(), e)
                    })?;
            }
            let module_entry = ctx
                .clone()
                .compile(format!("./{filename}"), script)
                .map_err(|e| format!("entry file compile failed: [{}]", e))?;

            let Ok(main) = module_entry
                .get("main")
                .unwrap_or_else(|_| module_entry.get::<&str, Function>("run"))
            else {
                return Err(r#"function "main" or "run" must exists"#.to_string());
            };

            // try run prehook, return if run failed
            if let Ok(prehook) = module_entry.get::<&str, Function>("prehook") {
                if let Err(e) = prehook.call_arg::<()>(Args::new(ctx.clone(), 0)) {
                    let msg = format!("prehook run failed: {}", e);
                    error!(msg);
                    return Err(msg);
                }
            }

            // continue if failed
            if let Err(e) = main.call_arg::<()>(Args::new(ctx.clone(), 0)) {
                error!("main run failed: {}", e)
            }

            // try run afterhook
            if let Ok(afterhook) = module_entry.get::<&str, Function>("afterhook") {
                if let Err(e) = afterhook.call_arg::<()>(Args::new(ctx.clone(), 0)) {
                    error!("afterhook run failed: {}", e);
                }
            }
            Ok(())
        })?;
        Ok(())
    }
}

const JS_IMPOR_PATTERN: &str = r#"[ 	]*import[ 	]+(.*)[ 	]+from[ 	]+('|")(\S+)('|")"#;

fn search_path(script: &str) -> Vec<String> {
    let re = regex::Regex::new(JS_IMPOR_PATTERN).unwrap();
    let mut paths = vec![];
    for (_, [_, _, path, _]) in re.captures_iter(script).map(|c| c.extract()) {
        paths.push(path.to_string());
    }
    paths
}

#[derive(Serialize, Deserialize, Debug)]
struct Response {
    code: i32,
    msg: String,
    data: String,
}

#[cfg(test)]
mod test {

    use rquickjs::{function::Args, Context, Runtime};

    fn get_context() -> rquickjs::Context {
        let runtime = Runtime::new().unwrap();

        Context::full(&runtime).unwrap()
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
