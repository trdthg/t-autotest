use crate::base::evloop::EventLoop;
use crate::base::tty::Tty;
use crate::term::Term;
use crate::ConsoleError;
use crate::Result;
use std::ops::Deref;
use std::ops::DerefMut;
use std::path::PathBuf;
use tracing::info;

pub struct Serial {
    inner: SerialClient<crate::VT102>,
}

impl Deref for Serial {
    type Target = Tty<crate::VT102>;

    fn deref(&self) -> &Self::Target {
        &self.inner.tty
    }
}

impl DerefMut for Serial {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner.tty
    }
}

impl Serial {
    pub fn new(c: t_config::ConsoleSerial) -> Result<Self> {
        let inner = Self::connect_from_serial_config(&c)?;

        Ok(Self { inner })
    }

    fn connect_from_serial_config(
        c: &t_config::ConsoleSerial,
    ) -> Result<SerialClient<crate::VT102>> {
        let ssh_client = SerialClient::connect(&c.serial_file, c.bund_rate, c.log_file.clone())?;
        Ok(ssh_client)
    }
}

struct SerialClient<T: Term> {
    pub tty: Tty<T>,
    pub path: String,
}

impl<T> SerialClient<T>
where
    T: Term,
{
    pub fn connect(file: &str, bund_rate: u32, log_file: Option<PathBuf>) -> Result<Self> {
        // init tty
        // t_util::execute_shell(
        //     format!("stty -F {} {} -echo -icrnl -onlcr -icanon", file, bund_rate).as_str(),
        // )
        // .map_err(ConsoleError::Stty)?;

        // connect serial
        let file = file.to_string();
        let evloop = EventLoop::spawn(
            move || {
                match serialport::new(&file, bund_rate).open() {
                    Ok(res) => {
                        info!("serial conn success");
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
            tty: Tty::new(evloop?),
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

    use crate::term::{Term, VT102};
    use std::{
        env,
        io::{ErrorKind, Read},
        thread::sleep,
        time::Duration,
    };

    use super::SerialClient;

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

        let port = serialport::new(serial.serial_file, serial.bund_rate)
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
        let c = t_config::load_config_from_file(f.unwrap()).map(Some);
        c.unwrap()
    }

    fn get_client(serial: &ConsoleSerial) -> SerialClient<VT102> {
        SerialClient::connect(&serial.serial_file, serial.bund_rate, None).unwrap()
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
