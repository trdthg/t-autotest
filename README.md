# [WIP] autotest

Autotest framework.

## Get Start

### Python(Recommend)

#### Install

just run: pip install xxx.wheel

Please check [Release](https://github.com/trdthg/t-autotest/releases)

#### Example

```py
import pyautotest
import pty
import os

class PseudoTTY():
    def __init__(self):
        self.master, self.slave = pty.openpty()
        self.pts = os.ttyname(self.slave)

    def get_pts(self):
        return self.pts

class RevShell(PseudoTTY):
    def __init__(self):
        super().__init__()
        shell_pid = os.fork()
        if shell_pid == 0:
            os.setsid()
            os.dup2(self.master, 0)
            os.dup2(self.master, 1)
            os.dup2(self.master, 2)
            os.close(self.slave)
            os.execv("/bin/sh", ["sh"])
        else:
            os.close(self.master)

shell = RevShell()

conf = f"""
    log_dir = "./logs"
    [env]
    [serial]
    serial_file = "{shell.get_pts()}"
    disable_echo = true
    """

print("pts:", shell.get_pts())

d = pyautotest.Driver(conf)
res = d.assert_script_run('whoami', 10)
print("whoami:", res)
d.stop()

```

### Cli(WIP)

Current provide Windows, Mac, Linux binary and python whl, auto build by Github Action. Download link:<https://github.com/trdthg/t-autotest/releases>

use binary directly after set env

you can also install with scripts:

#### linux / mac

```bash
curl -sSL https://github.com/trdthg/t-autotest/raw/main/scripts/install.sh | bash -
```

#### windows

```powershell
Invoke-WebRequest -Uri "https://github.com/trdthg/t-autotest/raw/main/scripts/install.ps1" -UseBasicParsing | Invoke-Expression
```

#### Usage

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


### Javascript(WIP)

#### Example

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

## More Example

## linebreak and echo

- some pts's echo is disabled, like sh
- some serial's linebreak is `\\r\\n`, like lpi4A(test with revyos), you should set linebreak to `\\r\\n`

```toml
log_dir = "./logs"
[env]
[serial]
serial_file = "/dev/ttyUSB0"
linebreak = "\\r\\n"
disable_echo = true
```
