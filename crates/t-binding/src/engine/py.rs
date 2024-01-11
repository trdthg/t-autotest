#[cfg(test)]
mod test {
    use pyo3::types::PyModule;

    #[test]
    fn test_pyo3() {
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
}
