#![allow(unused)]
use pyo3::{
    exceptions::{self, PyTypeError},
    prelude::*,
};
use t_binding::api;
use t_config::Config;
use t_runner::Runner;

/// Entrypoint, A Python module implemented in Rust.
#[pymodule]
fn pyautotest(py: Python, m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(sum_as_string, m)?)?;
    m.add_class::<Driver>()?;
    Ok(())
}

/// Formats the sum of two numbers as string.
#[pyfunction]
fn sum_as_string(a: usize, b: usize) -> PyResult<String> {
    Ok((a + b).to_string())
}

#[pyclass]
struct Driver {
    debug: bool,
}

#[pymethods]
impl Driver {
    #[new]
    fn __init__(debug: bool, config: Config) -> PyResult<Self> {
        let runner = Runner::new(config);
        runner.run();
        Ok(Self { debug })
    }

    /// Returns the sum of two numbers.
    #[staticmethod]
    fn hello() {
        println!("hello");
    }

    fn sleep(&self, miles: i32) {
        api::sleep(miles as u64);
    }

    fn get_env(&self, key: String) -> Option<String> {
        api::get_env(key)
    }

    fn assert_script_run_global(&self, cmd: String, timeout: i32) -> PyResult<String> {
        api::assert_script_run_global(cmd, timeout).ok_or(exceptions::PyAssertionError::new_err(
            "return code should be 0",
        ))
    }

    fn script_run_global(&self, cmd: String, timeout: i32) -> String {
        api::script_run_global(cmd, timeout).unwrap()
    }

    fn write_string(&self, s: String) {
        api::write_string(s);
    }

    // ssh
    fn ssh_assert_script_run_global(&self, cmd: String, timeout: i32) -> PyResult<String> {
        api::ssh_assert_script_run_global(cmd, timeout).ok_or(
            exceptions::PyAssertionError::new_err("return code should be 0"),
        )
    }

    fn ssh_script_run_global(&self, cmd: String, timeout: i32) -> String {
        api::ssh_script_run_global(cmd, timeout).unwrap()
    }

    fn ssh_write_string(&self, s: String) {
        api::ssh_write_string(s);
    }

    fn ssh_assert_script_run_seperate(&self, cmd: String, timeout: i32) -> Option<String> {
        api::ssh_assert_script_run_seperate(cmd, timeout)
    }

    // serial
    fn serial_assert_script_run_global(&self, cmd: String, timeout: i32) -> Option<String> {
        api::serial_assert_script_run_global(cmd, timeout)
    }

    fn serial_script_run_global(&self, cmd: String, timeout: i32) -> Option<String> {
        api::serial_script_run_global(cmd, timeout)
    }

    fn serial_write_string(&self, s: String) {
        api::serial_write_string(s);
    }

    // vnc
    fn assert_screen(&self, tag: String, timeout: i32) -> PyResult<()> {
        if !api::vnc_check_screen(tag, timeout) {
            Err(exceptions::PyAssertionError::new_err(
                "return code should be 0",
            ))
        } else {
            Ok(())
        }
    }

    fn check_screen(&self, tag: String, timeout: i32) -> bool {
        api::vnc_check_screen(tag, timeout)
    }

    fn mouse_click(&self) {
        api::vnc_mouse_click();
    }

    fn mouse_move(&self, x: i32, y: i32) {
        api::vnc_mouse_move(x as u16, y as u16);
    }

    fn mouse_hide(&self) {
        api::vnc_mouse_hide();
    }
}

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
