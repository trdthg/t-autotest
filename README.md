# [WIP] autotest

自动化测试框架

## Install

目前提供了 Windows, Mac, Linux 三平台二进制文件，由 Github Action 自动构建. 下载地址：<https://github.com/trdthg/t-autotest/releases>

下载完成后配置环境变量即可

如果你是 linux 系统, 则可以直接使用下面的脚本安装:

```bash
curl -sSL https://github.com/trdthg/t-autotest/blob/main/scripts/install-linux.sh | bash -
```

## Usage

使用方法: `autotest -f <config.toml> -c <case.ext>`

- `config.toml` 指定测试环境配置
- `case.ext` 指定需要运行的测试脚本, 目前支持 js 语言 

## 模块

- cli 模块 (提供命令行工具入口进程)
  - Feature
    - autotest: 提供命令行入口
- console 模块 (负责和机器终端交互)
  - ssh
    - 支持 private_key, password 登录
    - 在全局 shell session 交互式运行脚本
    - 在单独 shell session 运行命令
    - 等待 ssh tty 输出匹配文本
  - serial
    - 支持 password 登录
    - 在全局 session 交互式运行脚本
    - 等待 tty 输出匹配文本
    - 捕获所有串口输出文本，包括系统 boot 阶段 [输出参考](../doc/autotest/serial-log-example.txt)
  - vnc
    - 支持 vnc 连接，密码登录
    - 提供密码登录
- binding 模块 (负责测试脚本对接)
  - 提供基本的 api 函数
  - 集成到各个语言
    - js: 基于 quickjs 引擎完成 JS 测试脚本运行
    - python: TODO (pyO3)
- t-vnc 模块 
  - ([fork](https://github.com/trdthg/rust-vnc) 自 whitequark/rust-vnc, MIT)
  - 解决 windows 无法编译
- config 模块 (提供测试，命令行 需要的通用配置文件解析)
- util 模块 (工具库)
- ci (github action)
  - [`test.yaml`](https://github.com/trdthg/t-autotest/actions/workflows/test.yaml): 提交代码或 pr 时运行 cargo check, test, fmt, clippy, build(linux)
  - [`build.yaml`](https://github.com/trdthg/t-autotest/actions/workflows/release.yaml): 自动分发 linux, macos, windows 三平台二进制文件。[下载地址](https://github.com/trdthg/t-autotest/releases)

## api

- 通用
  - sleep: 为脚本提供统一的 sleep 函数实现
  - get_env: 获取 `config.toml` 定义的环境变量
  - assert_script_run: 根据配置文件自动选择 console, serial 优先于 ssh. 根据命令返回值判断，如果不为 0, 则会 panic
  - script_run: 同上，只运行命令，不处理返回值
  - write_string: 同上，只输入一段字符串，不包含控制字符
- ssh
  - ssh_assert_script_run_global: 调用 ssh 在主 session 执行脚本，断言命令返回值
  - ssh_script_run_seperate: 调用 ssh 在分离 session 执行脚本，其他同上
  - ssh_script_run_global: 调用 ssh 在主 session 执行脚本，只确保执行完成，不超时
  - ssh_write_string: 调用 ssh 在主 session 写入文本
- serial
  - serial_assert_script_run_global: 调用 serial 在主 session 执行脚本
  - serial_script_run_global: 调用 serial 在主 session 执行脚本，断言命令返回值
  - serial_write_string: 调用 serial 在主 session 执行脚本
- vnc
  - assert_screen: 调用 vnc 断言屏幕
  - check_screen: 调用 vnc 比较屏幕
  - mouse_click: 调用 vnc 鼠标点击
  - mouse_move: 调用 vnc 移动鼠标
  - mouse_hide: 调用 vnc 隐藏鼠标

## 测试用例示例

### ruyi 测试

- 测试用例：[ruyisdk.js](https://gitee.com/yan-mingzhu/autotest-examples/blob/master/ruyi/ruyisdk.js)

### poineerbox - riscv - debian

- 宿主机：wiondows
- 测试方法：ssh
- 配置文件：<https://gitee.com/yan-mingzhu/autotest-examples/blob/master/machine/poiner.toml>

### VF2 - riscv - ubuntu

- 宿主机：arch
- 测试方法：serial
- 配置文件：<https://gitee.com/yan-mingzhu/autotest-examples/blob/master/machine/VF2.toml>

### 树莓派 3B 1.2 - aarch - debian-bookworm

- 宿主机：nixos
- 测试方法：ssh + serial
- 配置文件：<https://gitee.com/yan-mingzhu/autotest-examples/blob/master/machine/rasp-pi.toml>
