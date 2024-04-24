#![allow(non_local_definitions)]
#![allow(unused)]

mod api;
use api::PyApi;
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
use t_binding::{
    api::{Api, ApiTx},
    ApiError, MsgReq, MsgRes,
};
use t_config::{Config, ConsoleSSH};
use t_console::SSH;
use t_runner::{Driver as InnerDriver, DriverBuilder};
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
fn pyautotest(py: Python, m: &Bound<'_, PyModule>) -> PyResult<()> {
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
    tx: ApiTx,
}

#[pymethods]
impl Driver {
    #[new]
    fn __init__(config: String) -> PyResult<Self> {
        let config =
            Config::from_toml_str(&config).map_err(|e| DriverException::new_err(e.to_string()))?;
        let mut driver = DriverBuilder::new(Some(config.clone()))
            .build()
            .map_err(|e| {
                DriverException::new_err(format!("driver init failed, reason: [{}]", e))
            })?;
        driver.start();
        Ok(Self {
            tx: driver.msg_tx.clone(),
            driver,
            config,
        })
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
        PyApi::new(&self.tx, py).sleep(miles as u64);
    }

    fn get_env(&self, py: Python<'_>, key: String) -> PyResult<Option<String>> {
        PyApi::new(&self.tx, py).get_env(key).map_err(into_pyerr)
    }

    fn assert_script_run(&self, py: Python<'_>, cmd: String, timeout: i32) -> PyResult<String> {
        PyApi::new(&self.tx, py)
            .assert_script_run(cmd, timeout)
            .map_err(into_pyerr)
    }

    fn script_run(&self, py: Python<'_>, cmd: String, timeout: i32) -> PyResult<(i32, String)> {
        PyApi::new(&self.tx, py)
            .script_run(cmd, timeout)
            .map_err(into_pyerr)
    }

    fn write(&self, py: Python<'_>, s: String) -> PyResult<()> {
        PyApi::new(&self.tx, py).write(s).map_err(into_pyerr)
    }

    fn writeln(&self, py: Python<'_>, s: String) -> PyResult<()> {
        PyApi::new(&self.tx, py)
            .write(format!("{s}\n"))
            .map_err(into_pyerr)
    }

    fn wait_string_ntimes(
        &self,
        py: Python<'_>,
        s: String,
        n: i32,
        timeout: i32,
    ) -> PyResult<bool> {
        PyApi::new(&self.tx, py)
            .wait_string_ntimes(s, n, timeout)
            .map_err(into_pyerr)
    }

    fn assert_wait_string_ntimes(
        &self,
        py: Python<'_>,
        s: String,
        n: i32,
        timeout: i32,
    ) -> PyResult<bool> {
        if !PyApi::new(&self.tx, py)
            .wait_string_ntimes(s, n, timeout)
            .map_err(into_pyerr)?
        {
            return Err(AssertException::new_err("wait failed"));
        }
        Ok(true)
    }

    // ssh
    fn ssh_assert_script_run(&self, py: Python<'_>, cmd: String, timeout: i32) -> PyResult<String> {
        PyApi::new(&self.tx, py)
            .ssh_assert_script_run(cmd, timeout)
            .map_err(into_pyerr)
    }

    fn ssh_script_run(&self, py: Python<'_>, cmd: String, timeout: i32) -> PyResult<(i32, String)> {
        PyApi::new(&self.tx, py)
            .ssh_script_run(cmd, timeout)
            .map_err(into_pyerr)
    }

    fn ssh_write(&self, py: Python<'_>, s: String) {
        PyApi::new(&self.tx, py).ssh_write(s);
    }

    fn ssh_assert_script_run_seperate(
        &self,
        py: Python<'_>,
        cmd: String,
        timeout: i32,
    ) -> PyResult<String> {
        PyApi::new(&self.tx, py)
            .ssh_assert_script_run_seperate(cmd, timeout)
            .map_err(into_pyerr)
    }

    // serial
    fn serial_assert_script_run(
        &self,
        py: Python<'_>,
        cmd: String,
        timeout: i32,
    ) -> PyResult<String> {
        PyApi::new(&self.tx, py)
            .serial_assert_script_run(cmd, timeout)
            .map_err(into_pyerr)
    }

    fn serial_script_run(
        &self,
        py: Python<'_>,
        cmd: String,
        timeout: i32,
    ) -> PyResult<(i32, String)> {
        PyApi::new(&self.tx, py)
            .serial_script_run(cmd, timeout)
            .map_err(into_pyerr)
    }

    fn serial_write(&self, py: Python<'_>, s: String) {
        PyApi::new(&self.tx, py).serial_write(s);
    }

    // vnc
    fn check_screen(&self, py: Python<'_>, tag: String, timeout: i32) -> PyResult<bool> {
        PyApi::new(&self.tx, py)
            .vnc_check_screen(tag, timeout)
            .map_err(into_pyerr)
    }

    fn assert_screen(&self, py: Python<'_>, tag: String, timeout: i32) -> PyResult<()> {
        PyApi::new(&self.tx, py)
            .vnc_assert_screen(tag, timeout)
            .map_err(into_pyerr)
    }

    fn type_string(&self, py: Python<'_>, s: String) -> PyResult<()> {
        PyApi::new(&self.tx, py)
            .vnc_type_string(s)
            .map_err(into_pyerr)
    }

    fn send_key(&self, py: Python<'_>, s: String) -> PyResult<()> {
        PyApi::new(&self.tx, py).vnc_send_key(s).map_err(into_pyerr)
    }

    fn vnc_refresh(&self, py: Python<'_>) -> PyResult<()> {
        PyApi::new(&self.tx, py).vnc_refresh().map_err(into_pyerr)
    }

    fn check_and_click(&self, py: Python<'_>, tag: String, timeout: i32) -> PyResult<bool> {
        PyApi::new(&self.tx, py)
            .vnc_check_and_click(tag, timeout)
            .map_err(into_pyerr)
    }

    fn assert_and_click(&self, py: Python<'_>, tag: String, timeout: i32) -> PyResult<()> {
        PyApi::new(&self.tx, py)
            .vnc_assert_and_click(tag, timeout)
            .map_err(into_pyerr)
    }

    fn mouse_click(&self, py: Python<'_>) -> PyResult<()> {
        PyApi::new(&self.tx, py)
            .vnc_mouse_click()
            .map_err(into_pyerr)
    }

    fn mouse_rclick(&self, py: Python<'_>) -> PyResult<()> {
        PyApi::new(&self.tx, py)
            .vnc_mouse_rclick()
            .map_err(into_pyerr)
    }

    fn mouse_keydown(&self, py: Python<'_>) -> PyResult<()> {
        PyApi::new(&self.tx, py)
            .vnc_mouse_keydown()
            .map_err(into_pyerr)
    }

    fn mouse_keyup(&self, py: Python<'_>) -> PyResult<()> {
        PyApi::new(&self.tx, py)
            .vnc_mouse_keyup()
            .map_err(into_pyerr)
    }

    fn mouse_move(&self, py: Python<'_>, x: i32, y: i32) -> PyResult<()> {
        PyApi::new(&self.tx, py)
            .vnc_mouse_move(x as u16, y as u16)
            .map_err(into_pyerr)
    }

    fn mouse_hide(&self, py: Python<'_>) -> PyResult<()> {
        PyApi::new(&self.tx, py)
            .vnc_mouse_hide()
            .map_err(into_pyerr)
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
    use pyo3::types::{PyAnyMethods, PyModule, PyModuleMethods};

    #[pyo3::pyfunction]
    fn add(a: i64, b: i64) -> i64 {
        // hello();
        a + b
    }

    #[test]
    fn test_pyo3() {
        pyo3::Python::with_gil(|py| -> pyo3::PyResult<()> {
            let module_name = "testapi".to_string();
            let module = PyModule::new_bound(py, &module_name)?;
            module.add_function(pyo3::wrap_pyfunction!(add, &module)?)?;

            // Import and get sys.modules
            let sys = PyModule::import_bound(py, "sys")?;
            let modules = sys.getattr("modules")?;
            let py_modules = modules.downcast::<pyo3::types::PyDict>()?;

            // Insert foo into sys.modules
            py_modules.set_item(&module_name, module)?;

            // Now we can import + run our python code
            pyo3::Python::run_bound(py, "import testapi; testapi.add(1, 2)", None, None).unwrap();

            // let res = py.eval("import testapi; testapi.add(1, 2)", None, None)?;
            // assert!(res.extract::<i64>()? == 4);
            Ok(())
        })
        .unwrap()
    }
}
