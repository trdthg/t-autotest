use crate::base::evloop::EventLoop;
use crate::base::tty::Tty;
use crate::base::tty::TtySetting;
use crate::term::Term;
use crate::ConsoleError;
use crate::Result;
use std::ops::Deref;
use std::ops::DerefMut;
use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::mpsc::Receiver;
use t_config::ConsoleSerialType;
use tracing::{error, info};

pub struct Serial {
    stop_tx: mpsc::Sender<()>,
    inner: Box<dyn SerialClient<crate::VT102> + Send + Sync>,
}

impl Deref for Serial {
    type Target = Tty<crate::VT102>;

    fn deref(&self) -> &Self::Target {
        self.inner.get_tty()
    }
}

impl DerefMut for Serial {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.inner.get_tty_mut()
    }
}

impl Serial {
    pub fn new(c: t_config::ConsoleSerial) -> Result<Self> {
        let (stop_tx, stop_rx) = mpsc::channel();

        let setting = TtySetting {
            disable_echo: c.disable_echo.unwrap_or(false),
            linebreak: c.linebreak.clone().unwrap_or("\n".to_string()),
        };

        #[cfg(never)]
        if setting.disable_echo {
            // init tty
            t_util::execute_shell(
                format!("stty -F {} echo -icrnl -onlcr -icanon", c.serial_file).as_str(),
            )
            .map_err(|_| ConsoleError::NoBashSupport("stty run failed".to_string()))?;
        }

        let inner: Box<dyn SerialClient<crate::VT102> + Send + Sync> = match c.r#type {
            #[cfg(target_os = "linux")]
            Some(ConsoleSerialType::Sock) => Box::new(SockClient::connect(
                &c.serial_file,
                c.log_file.clone(),
                stop_rx,
                setting,
            )?),
            _ => {
                let ssh_client = PtyClient::connect(
                    &c.serial_file,
                    c.bund_rate.unwrap_or(115200),
                    c.log_file.clone(),
                    stop_rx,
                    setting,
                )?;
                Box::new(ssh_client)
            }
        };
        Ok(Self { stop_tx, inner })
    }

    pub fn stop(&self) {
        if self.stop_tx.send(()).is_err() {
            error!("stop serial failed, serial may stopped already");
            return;
        }

        self.inner.get_tty().stop_evloop();
    }
}

trait SerialClient<T: Term> {
    fn get_tty(&self) -> &Tty<T>;
    fn get_tty_mut(&mut self) -> &mut Tty<T>;
}

impl<T: Term> SerialClient<T> for PtyClient<T> {
    fn get_tty(&self) -> &Tty<T> {
        &self.tty
    }

    fn get_tty_mut(&mut self) -> &mut Tty<T> {
        &mut self.tty
    }
}

#[cfg(target_os = "linux")]
impl<T: Term> SerialClient<T> for SockClient<T> {
    fn get_tty(&self) -> &Tty<T> {
        &self.tty
    }

    fn get_tty_mut(&mut self) -> &mut Tty<T> {
        &mut self.tty
    }
}

struct PtyClient<T: Term> {
    pub tty: Tty<T>,
    pub path: String,
}

impl<T> PtyClient<T>
where
    T: Term,
{
    pub fn connect(
        file: &str,
        bund_rate: u32,
        log_file: Option<PathBuf>,
        stop_rx: Receiver<()>,
        setting: TtySetting,
    ) -> Result<Self> {
        // connect serial
        let file = file.to_string();
        let evloop = EventLoop::spawn(
            move || {
                // disable echo

                match serialport::new(&file, bund_rate).open() {
                    Ok(res) => {
                        info!(msg = "serial conn success");
                        Ok(res)
                    }
                    Err(e) => {
                        // error!("serial conn failed: {}", e);
                        Err(ConsoleError::Serial(e))
                    }
                }
            },
            log_file,
        );

        Ok(Self {
            tty: Tty::new(evloop?, stop_rx, setting),
            path: "".to_string(),
        })
    }

    #[allow(unused)]
    pub fn tty(&self) -> String {
        self.path.to_owned()
    }
}

#[cfg(target_os = "linux")]
struct SockClient<T: Term> {
    #[allow(unused)]
    pub tty: Tty<T>,
    pub path: String,
}

#[cfg(target_os = "linux")]
impl<T> SockClient<T>
where
    T: Term,
{
    pub fn connect(
        file: &str,
        log_file: Option<PathBuf>,
        stop_rx: Receiver<()>,
        setting: TtySetting,
    ) -> Result<Self> {
        let file = file.to_string();

        let evloop = EventLoop::spawn(
            move || match std::os::unix::net::UnixStream::connect(std::path::Path::new(&file)) {
                Ok(res) => {
                    info!(msg = "serial(unix sock) conn success");
                    Ok(res)
                }
                Err(e) => {
                    error!("serial(unix sock) conn failed: {} {}", e, file);
                    Err(ConsoleError::IO(e))
                }
            },
            log_file,
        );

        Ok(Self {
            tty: Tty::new(evloop?, stop_rx, setting),
            path: "".to_string(),
        })
    }

    #[allow(unused)]
    pub fn tty(&self) -> String {
        self.path.to_owned()
    }
}

#[cfg(test)]
mod test {
    use t_config::{Config, ConsoleSerial};

    use crate::{
        base::tty::TtySetting,
        term::{Term, VT102},
    };
    use std::{
        env,
        io::{ErrorKind, Read},
        sync::mpsc::channel,
        thread::sleep,
        time::Duration,
    };

    use super::PtyClient;

    #[test]
    fn test_serial_boot() {
        let c = get_config_from_file();
        if c.is_none() {
            return;
        }
        let c = c.unwrap();
        let Some(serial) = c.serial else {
            return;
        };

        let port = serialport::new(serial.serial_file, serial.bund_rate.unwrap_or(115200))
            .timeout(Duration::from_millis(10))
            .open_native();
        if port.is_err() {
            return;
        }
        let mut port = port.unwrap();
        sleep(Duration::from_secs(20));
        loop {
            let mut buf = [0; 1024];
            match port.read(&mut buf) {
                Ok(n) => {
                    println!("{}", VT102::parse_and_strip(&buf[0..n]));
                }
                Err(e) if e.kind() == ErrorKind::TimedOut => {
                    println!("timeout");
                }
                Err(e) => {
                    eprintln!("panic, reason: [{}]", e);
                    panic!()
                }
            }
        }
    }

    fn get_config_from_file() -> Option<Config> {
        let f = env::var("AUTOTEST_CONFIG_FILE").ok();
        f.as_ref()?;
        t_config::load_config_from_file(f.unwrap()).ok()
    }

    fn get_client(serial: &ConsoleSerial) -> PtyClient<VT102> {
        let (_, rx) = channel();
        PtyClient::connect(
            &serial.serial_file,
            serial.bund_rate.unwrap_or(115200),
            None,
            rx,
            TtySetting {
                disable_echo: serial.disable_echo.unwrap_or(false),
                linebreak: serial.linebreak.clone().unwrap_or("\n".to_string()),
            },
        )
        .unwrap()
    }

    #[test]
    fn test_exec() {
        let Some(c) = get_config_from_file() else {
            return;
        };
        let Some(c) = c.serial else {
            return;
        };
        let mut serial = get_client(&c);

        let cmds = [
            ("unset A", ""),
            (r#"echo "A=$A""#, "A=\n"),
            ("export A=1", ""),
            (r#"echo "A=$A""#, "A=1\n"),
        ];

        (0..10).for_each(|_| {
            for cmd in cmds.iter() {
                let res = serial.tty.exec(Duration::from_secs(1), cmd.0).unwrap();
                assert_eq!(res.0, 0);
                assert_eq!(res.1, cmd.1);
            }
        })
    }
}
