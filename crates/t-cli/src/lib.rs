mod config;
mod runner;
use crate::config::{Console, ConsoleSSHAuthType};
pub use config::Config;
pub use runner::Runner;
use std::{
    path::Path,
    sync::{mpsc, Mutex, OnceLock},
    thread,
    time::Duration,
};
use t_binding::{MsgReq, MsgRes};
use t_console::{SSHAuthAuth, SSHClient, SerialClient, VNCClient};
use tracing::info;

static mut GLOBAL_SSH: OnceLock<SSHClient> = OnceLock::new();
static mut GLOBAL_SERIAL: OnceLock<SerialClient> = OnceLock::new();
static mut GLOBAL_VNC: OnceLock<Mutex<VNCClient>> = OnceLock::new();

pub fn init(config: Config) -> () {
    let Config {
        console:
            Console {
                ssh: _ssh,
                serial: _serial,
                vnc: _vnc,
            },
    } = config;
    info!("init...");
    if _ssh.enable {
        let auth = match _ssh.auth.r#type {
            ConsoleSSHAuthType::PrivateKey => SSHAuthAuth::PrivateKey(
                _ssh.auth.private_key.unwrap_or(
                    home::home_dir()
                        .map(|mut x| {
                            x.push(Path::new(".ssh/id_rsa"));
                            x.display().to_string()
                        })
                        .unwrap(),
                ),
            ),
            ConsoleSSHAuthType::Password => SSHAuthAuth::PrivateKey(_ssh.auth.password.unwrap()),
        };
        let ssh_client =
            SSHClient::connect(auth, _ssh.username, format!("{}:{}", _ssh.host, _ssh.port))
                .unwrap();
        if let Err(_) = unsafe { GLOBAL_SSH.set(ssh_client) } {
            panic!("ssh console init failed!");
        }
        info!("init ssh done");
    }

    if _serial.enable {
        let serial_console = SerialClient::connect(
            _serial.serial_file,
            _serial.bund_rate,
            Duration::from_secs(0),
        )
        .unwrap();
        if let Err(_) = unsafe { GLOBAL_SERIAL.set(serial_console) } {
            panic!("ssh console init failed!");
        }
        info!("init serial done");
    }

    if _vnc.enable {
        let vnc_client =
            VNCClient::connect(format!("{}:{}", _vnc.host, _vnc.port), _vnc.password).unwrap();
        if let Err(_) = unsafe { GLOBAL_VNC.set(Mutex::new(vnc_client)) } {
            panic!("ssh console init failed!");
        }
        info!("init vnc done");
    }

    let (tx, rx) = mpsc::channel();

    t_binding::init(tx);

    thread::spawn(move || {
        while let Ok((msg, tx)) = rx.recv() {
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
                    let res = unsafe { GLOBAL_SSH.get_mut().unwrap().exec_seperate(&cmd).unwrap() };
                    println!("{res}");
                    tx.send(MsgRes::AssertScriptRun { res }).unwrap();
                }
            }
        }
    });
}

#[cfg(test)]
mod test {
    use std::fs;

    use crate::Config;

    #[test]
    fn test_example_toml() {
        toml::from_str::<Config>(fs::read_to_string("./config.full.toml").unwrap().as_str())
            .unwrap();
    }
}
