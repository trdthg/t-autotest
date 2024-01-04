use std::{
    cell::{OnceCell, RefCell},
    collections::HashMap,
    rc::Rc,
    sync::{
        mpsc::{channel, Sender},
        Arc, Mutex,
    },
    thread,
    time::Duration,
};

use quick_js::{Context, JsValue};
use t_console::SSHClient;

use crate::{msg::MsgReq, msg::MsgRes, ScriptEngine};

pub struct JSEngine {
    cx: quick_js::Context,
}

impl ScriptEngine for JSEngine {}

impl JSEngine {
    fn _new() -> Self {
        let e = Self {
            cx: Context::new().unwrap(),
        };

        let mut ssh =
            SSHClient::connect("/home/trdthg/.ssh/id_rsa", "ecs-user", "47.94.225.51:22").unwrap();

        e.cx.add_callback("print", move |cmd: String| {
            println!("{cmd}");
            JsValue::Null
        })
        .unwrap();

        let (msg_tx_base, msg_rx) = channel::<(MsgReq, Sender<MsgRes>)>();

        let msg_tx = msg_tx_base.clone();
        e.cx.add_callback("assert_script_run", move |cmd: String, timeout: i32| {
            println!("assert_script_run pre");
            let (tx, rx) = channel::<MsgRes>();

            println!("assert_script_run sending");
            msg_tx
                .clone()
                .send((
                    MsgReq::AssertScriptRun {
                        cmd,
                        timeout: Duration::from_millis(timeout as u64),
                    },
                    tx,
                ))
                .unwrap();
            println!("assert_script_run send done");

            println!("assert_script_run waiting");
            let res = rx
                .recv_timeout(Duration::from_millis(timeout as u64))
                .unwrap();

            let res = if let MsgRes::AssertScriptRun { res } = res {
                JsValue::String(res)
            } else {
                JsValue::Null
            };
            println!("assert_script_run done");
            res
        })
        .unwrap();

        let msg_tx = msg_tx_base.clone();
        e.cx.add_callback("assert_screen", move |tags: String, timeout: i32| {
            println!("assert_script_run pre");
            let (tx, rx) = channel::<MsgRes>();

            println!("assert_script_run sending");
            msg_tx
                .send((
                    MsgReq::AssertScreen {
                        tag: tags,
                        threshold: 1,
                        timeout: Duration::from_millis(timeout as u64),
                    },
                    tx,
                ))
                .unwrap();
            println!("assert_script_run send done");

            println!("assert_script_run waiting");
            let res = rx
                .recv_timeout(Duration::from_millis(timeout as u64))
                .unwrap();

            let res = if let MsgRes::AssertScreen { similarity, ok } = res {
                JsValue::Int(similarity)
            } else {
                JsValue::Null
            };
            println!("assert_script_run done");
            res
        })
        .unwrap();

        thread::spawn(move || {
            while let Ok((msg, tx)) = msg_rx.recv() {
                println!("recv");
                match msg {
                    MsgReq::AssertScreen {
                        tag,
                        threshold,
                        timeout,
                    } => {
                        tx.send(MsgRes::AssertScreen {
                            similarity: 1,
                            ok: true,
                        })
                        .unwrap();
                    }
                    MsgReq::AssertScriptRun { cmd, .. } => {
                        let res = ssh.exec(&cmd).unwrap();
                        println!("{res}");
                        tx.send(MsgRes::AssertScriptRun { res }).unwrap();
                    }
                }
            }
        });

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
