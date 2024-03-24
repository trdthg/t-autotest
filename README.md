# [WIP] autotest

Autotest framework.

## Install

Current provide Windows, Mac, Linux binary and python whl, auto build by Github Action. Download link:<https://github.com/trdthg/t-autotest/releases>

use binary directly after set env

you can also install with scripts:

### linux / mac

```bash
curl -sSL https://github.com/trdthg/t-autotest/blob/main/scripts/install.sh | bash -
```

### windows

```powershell
Invoke-WebRequest -Uri "https://github.com/trdthg/t-autotest/raw/main/scripts/install.ps1" -UseBasicParsing | Invoke-Expression
```

## Usage

```txt
Usage: autotest --config <CONFIG> <COMMAND>

Commands:
  run
  record
  vnc-do
  help    Print this message or the help of the given subcommand(s)

Options:
  -c, --config <CONFIG>
  -h, --help             Print help
```

## Examples

### use as python pkg

```py
import pyautotest

if __name__ == "__main__":
    d = pyautotest.Driver(
        """
        log_dir = "./logs"
        [env]
        [serial]
        serial_file = "/dev/ttyUSB0"
        bund_rate   = 115200
        """
    )

    d.writeln("\x03")
    d.assert_wait_string_ntimes("login", 1, 10)

    d.sleep(3)
    d.writeln("pi")
    d.assert_wait_string_ntimes("Password", 1, 10)

    d.sleep(3)
    d.writeln("pi")

    d.sleep(3)
    res = d.assert_script_run("whoami", 5)
```

### use as js script

```js
import { add } from "./lib.js"

export function prehook() {
    let res = script_run("ls", 9000)
    console.log(res);
}

export function run() {
    let res = script_run("lsa", 9000)
    console.log(res);
}

export function afterhook() {
    let res = script_run("ls", 9000)
    console.log(res);
}
```

## Module

- cli (cli entry)
- console (interact with os console)
  - ssh
  - serial
  - vnc
- binding
  - provide stateless api func
  - binding
    - js: based on quickjs
    - python: pyO3
- t-vnc
  - [fork](https://github.com/trdthg/rust-vnc) from whitequark/rust-vnc, MIT
- config
- ci (github action)
  - [`test.yaml`](https://github.com/trdthg/t-autotest/actions/workflows/test.yaml): cargo check, test, fmt, clippy, build(linux)
  - [`build.yaml`](https://github.com/trdthg/t-autotest/actions/workflows/release.yaml): auto release linux, macos, windows binary and python whl。[Download](https://github.com/trdthg/t-autotest/releases)

## api

```py
class Driver:
    """
    A driver for running test

    :param toml_str: toml config string
    """

    def __init__(self, toml_str: str) -> Driver: ...
    def start(self):
        """
        start the runner
        """

    def stop(self):
        """
        stop the runner
        """

    def sleep(self, secs: int):
        """
        sleep for secs, you can use this function to simulate a long running script
        """

    def get_env(self, key: str) -> str | None:
        """
        get environment variable by key from toml env section
        """

    def assert_script_run(self, cmd: str, timeout: int) -> str:
        """
        run script in console, return stdout, throw exception if return code is not 0
        """

    def script_run(self, cmd: str, timeout: int) -> str:
        """
        like assert_script_run, but not throw exception if return code is not 0
        """

    def write(self, s: str):
        """
        write string to console
        """

    def writeln(self, s: str):
        """
        write string with '\n' to console
        """

    def wait_string_ntimes(self, s: str, n: int, timeout: int) -> bool:
        """
        wait pattern in console output show n times
        """

    def assert_wait_string_ntimes(self, s: str, n: int, timeout: int):
        """
        wait pattern in console output, if timeout, throw error
        """

    def ssh_assert_script_run(self, cmd: str, timeout: int) -> str:
        """
        run script in ssh, return stdout, throw exception if return code is not 0
        """

    def ssh_script_run(self, cmd: str, timeout: int) -> str:
        """
        like ssh_assert_script_run, but not throw exception if return code is not 0
        """

    def ssh_write(self, s: str):
        """
        write string to ssh console
        """

    def ssh_assert_script_run_seperate(self, cmd: str, timeout: int) -> str:
        """
        run script in seperate ssh session, return stdout, throw exception if return code is not 0
        """

    def serial_assert_script_run(self, cmd: str, timeout: int) -> str:
        """
        run script in global ssh session, return stdout, throw exception if return code is not 0
        """

    def serial_script_run(self, cmd: str, timeout: int) -> str:
        """
        like serial_assert_script_run, but not throw exception if return code is not 0
        """

    def serial_write(self, s: str):
        """
        write string to ssh console
        """

    def assert_screen(self, tag: str, timeout: int):
        """
        check screen, throw exception if timeout, or not similar to tag
        """

    def check_screen(self, tag: str, timeout: int) -> bool:
        """
        check screen, return false if timeout, or not similar to tag
        """

    def vnc_type_string(self, s: str):
        """
        type string
        """

    def vnc_send_key(self):
        """
        send event
        """

    def vnc_refresh(self):
        """
        force refresh
        """

    def mouse_click(self):
        """
        click mouse
        """

    def mouse_rclick(self):
        """
        click mouse right button
        """

    def mouse_keydown(self):
        """
        mouse left button down
        """

    def mouse_keyup(self):
        """
        mouse left button up
        """

    def mouse_move(self, x: int, y: int):
        """
        move mouse to x, y
        """

    def mouse_hide(self):
        """
        hide mouse
        """
```
