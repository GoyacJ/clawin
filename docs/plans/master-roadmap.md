# Clawin 主路线图

- 状态: Active
- 基线日期: 2026-04-09
- 当前进度: `M7` / `Phase 8E release gate hardening in progress`

## 总目标

以 Rust 重构 Claude Code `2.1.88` 公开包能力，形成可发布、可验证、可扩展的 `clawin`。

## 阶段划分

### Phase 0: 文档与治理基线

- 目标
  - 建立宪章、对标矩阵、ADR、工程标准、路线图、状态文档
- 入口条件
  - 仓库存在上游 sourcemap 参考语料
- 退出条件
  - 文档体系完整，可作为后续实施基线

### Phase 1: Workspace 与基础装配

- 目标
  - 建立 Cargo workspace、基础 crate、CI 与 lint/test 骨架
- 入口条件
  - Phase 0 完成
- 退出条件
  - 能构建空骨架，基础依赖与 crate 边界稳定

### Phase 2: Config / Bootstrap

- 目标
  - 实现 `clawin-bootstrap`、`clawin-config` 的首轮能力
- 范围
  - 启动、路径归一化、全局/项目配置、schema version、migration 框架
- 退出条件
  - bootstrap/config 子系统达到可测试、可演示、可继续扩展的状态

### Phase 3: Tools / Commands 基础设施

- 目标
  - 建立 tool registry、command registry、permission 基础链路
- 退出条件
  - 能在无完整 engine 的前提下跑通命令和工具骨架

### Phase 4: Conversation Engine

- 目标
  - 实现 turn loop、streaming、tool orchestration、compact、budget
- 退出条件
  - 能跑通最小端到端会话

### Phase 5: TUI / REPL

- 目标
  - 重建终端交互体验
- 退出条件
  - REPL 基本可用，交互/非交互路径分离稳定

### Phase 6A: MCP

- 目标
  - 补齐 stdio MCP 协议接入、bootstrap 装配、REPL 状态查看、engine/tool 主链路调用
- 退出条件
  - stdio MCP 可加载、可重载、可展示状态、可调用远端 tool、可读取 resources

### Phase 6B: Skills / Plugins

- 目标
  - 补齐 skills/plugins 的内容加载、命令扩展、约束注入与运行时装配
- 退出条件
  - skills/plugins 加载规则可用，并与 MCP 主链路共存

### Phase 7A: Worktree / Session / Resume

- 目标
  - 收口本地 session persistence、resume/continue、git-backed session-owned worktree
- 退出条件
  - `bootstrap -> engine -> REPL` 主链路可写入/恢复本地 JSONL transcript，可显式 resume/continue，并可进入/退出 session-owned worktree

### Phase 7B1: Structured IO / Headless

- 目标
  - 收口本地 stdio JSON 的 headless 非交互闭环，复用现有 `bootstrap -> engine -> tools -> session persistence` 主链路
- 退出条件
  - `clawin --print` 可稳定支持 `text/json/stream-json` 输出、fresh/`--continue`/`--resume`、host-mediated permission request/response、interrupt 与跨 submit transcript 保留

### Phase 7B2: Remote Control / Bridge

- 目标
  - 收口 remote bridge、远程 transport 与多客户端协作相关能力
- 退出条件
  - remote-control/bridge 关键流程具备端到端行为闭环，并可与 `Phase 7B1` 的 structured IO 基线复用协议与 session 恢复能力

### Phase 8: Parity Hardening 与发布准备

- 目标
  - 完成 golden fixture、三平台矩阵、差异审查、发布检查表
- 退出条件
  - 满足 V1 发布门槛

当前 Phase 8 的执行口径固定为：

- 先以 `docs/claude-code-sourcemap-main/restored-src/src/` 重新对照现有一级子系统，确认哪些最小实现已经足以构成 V1 闭环，哪些仍只是样本实现
- 在一级子系统没有形成 `Parity Verified` 或 `Accepted Difference` 证据前，仓库继续保持 `M7`
- 只有在三平台验证证据、golden / smoke / failure-path 证据和发布检查表同时齐备后，才允许切换到 `M8`

当前 `Phase 8E` 的执行批次固定为：

- 冻结阶段性 hardening 基线，不再引入新的功能面
- 先归档固定 smoke 组的本地 macOS 结果，并继续等待 Linux/Windows CI 结果进入 release gate
- 当前远端仅有 `origin/main`，且 `origin/main` 尚无 `rust-ci.yml`；在包含 fixed smoke matrix 的基线 push/merge 到远端前，Linux/Windows release archive 无法生成
- 以发布检查表和三平台 smoke matrix 收口 `M8` 门禁
- 逐一级子系统给出正式发布结论：`Parity Verified`、`Parity Pending` 或 `Accepted Difference`；首批四个高优先级子系统已完成正式复核，但在远端三平台证据缺位前继续保持 `Parity Pending`
- 任一门禁未收口前继续保持 `M7`

## 里程碑

| 里程碑 | 定义 |
| --- | --- |
| `M0` | 文档治理体系建立 |
| `M1` | Rust workspace 可构建 |
| `M2` | bootstrap/config 通过首轮试运行 |
| `M3` | commands/tools 基础设施与最小 session 装配可用 |
| `M4` | conversation engine 最小端到端会话可用 |
| `M5` | TUI / REPL 最小交互闭环可用 |
| `M6` | `Phase 6A + Phase 6B` 全部完成，MCP / skills / plugins 主链路可用 |
| `M7` | `Phase 7A + Phase 7B1 + Phase 7B2` 全部完成，worktree / session / resume / structured IO / remote 关键流程可用 |
| `M8` | 三平台对标验证与发布准备完成 |

## 关键依赖

- 上游公开包行为样本与 fixture 收集
- 三平台持续集成环境
- 可稳定执行的 golden fixture 测试框架
- git/worktree/terminal 相关平台验证环境

## 当前建议迁移顺序

1. `config` + `bootstrap`
2. `tools` + `commands`
3. `engine`
4. `ui`
5. `mcp`
6. `skills/plugins`
7. `worktree` + `session/resume`
8. `structured IO / headless`
9. `remote control / bridge`

## 风险门槛

若出现以下情况，必须暂停扩展实现并优先治理:

- crate 边界开始失真
- 对标矩阵无法支持评审
- 差异记录失控
- Windows 平台长期无人验证
- 核心行为只能靠人工口头解释
