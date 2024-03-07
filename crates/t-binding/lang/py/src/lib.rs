#![allow(unused)]
use pyo3::{
    exceptions::{self, PyException, PyTypeError},
    prelude::*,
};
use std::{env, time::Duration};
use t_binding::api;
use t_config::{Config, ConsoleSSH};
use t_console::SSH;
use t_runner::Driver as InnerDriver;
use tracing::Level;
use tracing_subscriber::FmtSubscriber;

pyo3::create_exception!(defaultmodule, StringError, PyException);
pyo3::create_exception!(defaultmodule, AssertError, PyException);
pyo3::create_exception!(defaultmodule, TimeoutError, PyException);
pyo3::create_exception!(defaultmodule, UnexpectedError, PyException);

/// Entrypoint, A Python module implemented in Rust.
#[pymodule]
fn pyautotest(py: Python, m: &PyModule) -> PyResult<()> {
    ctrlc::set_handler(|| std::process::exit(2)).unwrap();
    init_logger();

    tracing::info!("pyautotest module initialized");
    m.add_class::<Driver>()?;
    Ok(())
}

fn init_logger() {
    let format = tracing_subscriber::fmt::format()
        .without_time()
        .with_target(false)
        .with_level(true)
        .with_source_location(true)
        .compact();

    let subscriber = FmtSubscriber::builder()
        .with_max_level(match env::var("RUST_LOG") {
            Ok(l) => match l.as_str() {
                "trace" => Level::TRACE,
                "debug" => Level::DEBUG,
                "warn" => Level::WARN,
                "error" => Level::ERROR,
                _ => Level::INFO,
            },
            _ => Level::INFO,
        })
        .event_format(format)
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");
}

#[pyclass]
struct Driver {
    inner: InnerDriver,
}

#[pymethods]
impl Driver {
    #[new]
    fn __init__(config: String) -> PyResult<Self> {
        let config =
            Config::from_toml_str(&config).map_err(|e| StringError::new_err(e.to_string()))?;
        let mut runner = InnerDriver::new(config.clone());
        runner.start();
        Ok(Self { inner: runner })
    }

    // ssh
    fn new_ssh(&self) -> DriverSSH {
        DriverSSH::new(self.inner.config.console.ssh.clone())
    }

    fn stop(&mut self) {
        self.inner.stop();
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

    fn script_run_global(&self, cmd: String, timeout: i32) -> PyResult<String> {
        match api::script_run_global(cmd, timeout) {
            None => Err(TimeoutError::new_err("wait output timeout")),
            Some(v) => Ok(v),
        }
    }

    fn write_string(&self, s: String) {
        api::write_string(s);
    }

    fn wait_string_ntimes(&self, s: String, n: i32, timeout: i32) {
        api::wait_string_ntimes(s, n, timeout)
    }

    // ssh
    fn ssh_assert_script_run_global(&self, cmd: String, timeout: i32) -> PyResult<String> {
        api::ssh_assert_script_run_global(cmd, timeout).ok_or(
            exceptions::PyAssertionError::new_err("return code should be 0, or timeout"),
        )
    }

    fn ssh_script_run_global(&self, cmd: String, timeout: i32) -> PyResult<String> {
        api::ssh_script_run_global(cmd, timeout)
            .ok_or(exceptions::PyAssertionError::new_err("timeout"))
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

#[pyclass]
struct DriverSSH {
    inner: SSH,
}

impl DriverSSH {
    pub fn new(c: ConsoleSSH) -> Self {
        Self { inner: SSH::new(c) }
    }
}

#[pymethods]
impl DriverSSH {
    fn get_tty(&self) -> String {
        self.inner.tty()
    }

    fn assert_script_run(&mut self, cmd: String, timeout: u64) -> PyResult<String> {
        let Ok(v) = self.inner.exec_global(Duration::from_millis(timeout), &cmd) else {
            return Err(TimeoutError::new_err("assert script run timeout"));
        };
        if v.0 != 0 {
            return Err(AssertError::new_err(format!(
                "return code should be 0, but got {}",
                v.0
            )));
        }
        Ok(v.1)
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
