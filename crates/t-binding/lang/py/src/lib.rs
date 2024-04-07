#![allow(unused)]
mod api;
use pyo3::{
    exceptions::{self, PyException, PyTypeError},
    prelude::*,
};
use std::{
    env,
    sync::{
        mpsc::{Receiver, Sender},
        Arc, Mutex,
    },
    time::Duration,
};
use t_binding::ApiError;
use t_config::{Config, ConsoleSSH};
use t_console::SSH;
use t_runner::{Driver as InnerDriver, Server};
use tracing::{error, Level};
use tracing_subscriber::FmtSubscriber;

pyo3::create_exception!(defaultmodule, DriverException, PyException);
pyo3::create_exception!(defaultmodule, UserException, PyException);
pyo3::create_exception!(defaultmodule, AssertException, PyException);
pyo3::create_exception!(defaultmodule, TimeoutException, PyException);
pyo3::create_exception!(defaultmodule, UnexpectedException, PyException);

fn into_pyerr(e: ApiError) -> PyErr {
    match e {
        ApiError::ServerStopped => DriverException::new_err("server stopped"),
        ApiError::ServerInvalidResponse => {
            DriverException::new_err("server return invalid response, please open an issue")
        }
        ApiError::String(s) => UnexpectedException::new_err(s),
        ApiError::Timeout => TimeoutException::new_err("timeout"),
        ApiError::AssertFailed => AssertException::new_err("assert failed"),
        ApiError::Interrupt => UserException::new_err("interrupted by user"),
    }
}

/// Entrypoint, A Python module implemented in Rust.
#[pymodule]
fn pyautotest(py: Python, m: &PyModule) -> PyResult<()> {
    init_logger();

    tracing::info!("pyautotest module initialized");
    m.add_class::<Driver>()?;
    Ok(())
}

fn init_logger() {
    let log_level = match env::var("RUST_LOG") {
        Ok(l) => match l.as_str() {
            "trace" => Level::TRACE,
            "debug" => Level::DEBUG,
            "warn" => Level::WARN,
            "error" => Level::ERROR,
            "info" => Level::INFO,
            _ => return,
        },
        _ => Level::INFO,
    };

    let format = tracing_subscriber::fmt::format()
        .without_time()
        .with_target(false)
        .with_level(true)
        .with_source_location(true)
        .compact();

    let subscriber = FmtSubscriber::builder()
        .with_max_level(log_level)
        .event_format(format)
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");
}

#[pyclass]
struct Driver {
    config: Config,
    driver: InnerDriver,
}

#[pymethods]
impl Driver {
    #[new]
    fn __init__(config: String) -> PyResult<Self> {
        let config =
            Config::from_toml_str(&config).map_err(|e| DriverException::new_err(e.to_string()))?;
        let mut driver = InnerDriver::new(config.clone()).map_err(|e| {
            DriverException::new_err(format!("driver init failed, reason: [{}]", e))
        })?;
        driver.start();
        Ok(Self { driver, config })
    }

    // ssh
    fn new_ssh(&self) -> PyResult<DriverSSH> {
        let Some(ssh) = self.config.ssh.clone() else {
            return Err(DriverException::new_err("no ssh config"));
        };
        DriverSSH::new(ssh)
    }

    fn stop(&mut self) {
        self.driver.stop();
    }

    fn sleep(&self, py: Python<'_>, miles: i32) {
        api::sleep(py, miles as u64);
    }

    fn get_env(&self, py: Python<'_>, key: String) -> PyResult<Option<String>> {
        api::get_env(py, key).map_err(into_pyerr)
    }

    fn assert_script_run(&self, py: Python<'_>, cmd: String, timeout: i32) -> PyResult<String> {
        api::assert_script_run(py, cmd, timeout).map_err(into_pyerr)
    }

    fn script_run(&self, py: Python<'_>, cmd: String, timeout: i32) -> PyResult<(i32, String)> {
        api::script_run(py, cmd, timeout).map_err(into_pyerr)
    }

    fn write(&self, py: Python<'_>, s: String) -> PyResult<()> {
        api::write(py, s).map_err(into_pyerr)
    }

    fn writeln(&self, py: Python<'_>, s: String) -> PyResult<()> {
        api::write(py, format!("{s}\n")).map_err(into_pyerr)
    }

    fn wait_string_ntimes(
        &self,
        py: Python<'_>,
        s: String,
        n: i32,
        timeout: i32,
    ) -> PyResult<bool> {
        api::wait_string_ntimes(py, s, n, timeout).map_err(into_pyerr)
    }

    fn assert_wait_string_ntimes(
        &self,
        py: Python<'_>,
        s: String,
        n: i32,
        timeout: i32,
    ) -> PyResult<bool> {
        if !api::wait_string_ntimes(py, s, n, timeout).map_err(into_pyerr)? {
            return Err(AssertException::new_err("wait failed"));
        }
        Ok(true)
    }

    // ssh
    fn ssh_assert_script_run(&self, py: Python<'_>, cmd: String, timeout: i32) -> PyResult<String> {
        api::ssh_assert_script_run(py, cmd, timeout).map_err(into_pyerr)
    }

    fn ssh_script_run(&self, py: Python<'_>, cmd: String, timeout: i32) -> PyResult<(i32, String)> {
        api::ssh_script_run(py, cmd, timeout).map_err(into_pyerr)
    }

    fn ssh_write(&self, py: Python<'_>, s: String) {
        api::ssh_write(py, s);
    }

    fn ssh_assert_script_run_seperate(
        &self,
        py: Python<'_>,
        cmd: String,
        timeout: i32,
    ) -> PyResult<String> {
        api::ssh_assert_script_run_seperate(py, cmd, timeout).map_err(into_pyerr)
    }

    // serial
    fn serial_assert_script_run(
        &self,
        py: Python<'_>,
        cmd: String,
        timeout: i32,
    ) -> PyResult<String> {
        api::serial_assert_script_run(py, cmd, timeout).map_err(into_pyerr)
    }

    fn serial_script_run(
        &self,
        py: Python<'_>,
        cmd: String,
        timeout: i32,
    ) -> PyResult<(i32, String)> {
        api::serial_script_run(py, cmd, timeout).map_err(into_pyerr)
    }

    fn serial_write(&self, py: Python<'_>, s: String) {
        api::serial_write(py, s);
    }

    // vnc
    fn assert_screen(&self, py: Python<'_>, tag: String, timeout: i32) -> PyResult<()> {
        api::vnc_assert_screen(py, tag, timeout).map_err(into_pyerr)
    }

    fn vnc_type_string(&self, py: Python<'_>, s: String) -> PyResult<()> {
        api::vnc_type_string(py, s).map_err(into_pyerr)
    }

    fn vnc_send_key(&self, py: Python<'_>, s: String) -> PyResult<()> {
        api::vnc_send_key(py, s).map_err(into_pyerr)
    }

    fn vnc_refresh(&self, py: Python<'_>) -> PyResult<()> {
        api::vnc_refresh(py).map_err(into_pyerr)
    }

    fn check_screen(&self, py: Python<'_>, tag: String, timeout: i32) -> PyResult<bool> {
        api::vnc_check_screen(py, tag, timeout).map_err(into_pyerr)
    }

    fn mouse_click(&self, py: Python<'_>) -> PyResult<()> {
        api::vnc_mouse_click(py).map_err(into_pyerr)
    }

    fn mouse_rclick(&self, py: Python<'_>) -> PyResult<()> {
        api::vnc_mouse_rclick(py).map_err(into_pyerr)
    }

    fn mouse_keydown(&self, py: Python<'_>) -> PyResult<()> {
        api::vnc_mouse_keydown(py).map_err(into_pyerr)
    }

    fn mouse_keyup(&self, py: Python<'_>) -> PyResult<()> {
        api::vnc_mouse_keyup(py).map_err(into_pyerr)
    }

    fn mouse_move(&self, py: Python<'_>, x: i32, y: i32) -> PyResult<()> {
        api::vnc_mouse_move(py, x as u16, y as u16).map_err(into_pyerr)
    }

    fn mouse_hide(&self, py: Python<'_>) -> PyResult<()> {
        api::vnc_mouse_hide(py).map_err(into_pyerr)
    }
}

#[pyclass]
struct DriverSSH {
    inner: SSH,
}

impl DriverSSH {
    pub fn new(c: ConsoleSSH) -> PyResult<Self> {
        Ok(Self {
            inner: SSH::new(c).map_err(|e| DriverException::new_err(e.to_string()))?,
        })
    }
}

#[pymethods]
impl DriverSSH {
    fn get_tty(&self) -> String {
        self.inner.tty()
    }

    fn assert_script_run(&mut self, cmd: String, timeout: u64) -> PyResult<String> {
        let Ok(v) = self.inner.exec(Duration::from_secs(timeout), &cmd) else {
            return Err(TimeoutException::new_err("assert script run timeout"));
        };
        if v.0 != 0 {
            return Err(AssertException::new_err(format!(
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
