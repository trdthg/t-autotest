use pyo3::types::PyModule;

#[test]
fn test_quickjs() {
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
fn test_mlua() {
    let lua = mlua::Lua::new();

    let map_table = lua.create_table().unwrap();
    map_table.set(1, "one").unwrap();
    map_table.set("two", 2).unwrap();
    lua.globals().set("map_table", map_table).unwrap();

    let hello_str: &'static str = "hello";
    let hello = move || {
        println!("{}", hello_str);
    };
    lua.globals()
        .set(
            "rust_add",
            lua.create_function(move |_, (a, b): (i32, i32)| -> mlua::Result<i32> {
                hello();
                Ok(a + b)
            })
            .unwrap(),
        )
        .unwrap();

    let res = lua
        .load(
            r"
                for k,v in pairs(map_table) do print(k,v) end
                return rust_add(1, 2)
            ",
        )
        .eval::<i32>()
        .unwrap();
    assert!(res == 3);
}

#[test]
fn test_pyo3() {
    let hello_str = "hello";
    let hello = move || {
        println!("{}", hello_str);
    };

    #[pyo3::pyfunction]
    fn add(a: i64, b: i64) -> i64 {
        // hello();
        a + b
    }

    pyo3::Python::with_gil(|py| -> pyo3::PyResult<()> {
        let module_testapi_name = "testapi".to_string();
        let module_testapi = PyModule::new(py, &module_testapi_name)?;
        module_testapi.add_function(pyo3::wrap_pyfunction!(add, module_testapi)?)?;

        // Import and get sys.modules
        let sys = PyModule::import(py, "sys")?;
        let py_modules: &pyo3::types::PyDict = sys.getattr("modules")?.downcast()?;

        // Insert foo into sys.modules
        py_modules.set_item(&module_testapi_name, module_testapi)?;

        // Now we can import + run our python code
        pyo3::Python::run(py, "import testapi; testapi.add(1, 2)", None, None).unwrap();

        // let res = py.eval("import testapi; testapi.add(1, 2)", None, None)?;
        // assert!(res.extract::<i64>()? == 4);
        Ok(())
    })
    .unwrap()
}
