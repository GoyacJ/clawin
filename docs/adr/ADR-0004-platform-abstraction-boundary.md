# ADR-0004: 平台抽象边界

- 状态: Accepted
- 日期: 2026-04-08

## 背景

Clawin V1 要三平台齐发。若平台差异散落在命令、工具、engine、UI 各处，后续维护成本会迅速失控。

## 决策

所有 OS 差异必须收敛到 `clawin-platform`。

`clawin-platform` 负责以下能力:

- shell 执行与进程管理
- TTY 能力探测
- 路径归一化与大小写/分隔符差异
- secure storage
- 浏览器打开与外部程序调用
- terminal profile / backup / system integration
- 音频、输入、未来必须的原生能力

其他 crate 只能依赖平台 trait，例如:

- `ShellAdapter`
- `SecureStorage`
- `TerminalCapabilities`
- `PathPolicy`
- `BrowserLauncher`

禁止以下做法:

- 在业务 crate 中直接写 `cfg(target_os)` 分支来处理产品逻辑
- 在 UI 或 engine 中直接调用平台专有系统 API
- 让测试逻辑依赖真实 OS 副作用

## 后果

- 优点: 平台问题定位更集中，测试替身更容易实现
- 成本: 需要设计更完整的适配接口
- 影响: 任何平台新能力都要先扩展 adapter，再进入业务层
