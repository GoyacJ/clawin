# 对标进度总览

- 更新时间: 2026-04-09
- 当前里程碑: `M7`
- 当前执行阶段: `Phase 7 complete`

## 总体状态

文档与治理基线、Cargo workspace、最小 `clawin` 可执行骨架、`bootstrap/config` 首轮闭环、`commands/tools/engine` 的 `M4` 最小端到端会话闭环、`Phase 5` 的最小 `TUI / REPL` 交互闭环，以及 `Phase 6A + 6B` 的 MCP / skills / plugins 最小主链路闭环均已落地。仓库随后完成了 `Phase 7A: Worktree / Session / Resume`、`Phase 7B1: Structured IO / Headless` 与 `Phase 7B2: Remote Control / Bridge`：`clawin` 现已同时具备本地 JSONL session 持久化与恢复、git-backed session-owned worktree、显式 `--print` headless 结构化 IO，以及 `remote-control` / `/remote-control` 桥接当前会话的最小闭环。当前里程碑正式切到 `M7`，后续进入 `Phase 8` 的 parity hardening 与发布准备。

## 子系统状态

| 子系统 | 当前状态 | 下一步 |
| --- | --- | --- |
| Bootstrap / Entrypoint | `Parity Pending` | 在现有 startup/config 装配之上补充 commands/tools 入口与更多行为 fixture |
| Config / Settings / Persistence | `Parity Pending` | 在现有 schema/migration 闭环之上继续补 settings 合并语义与更多对标样本 |
| Commands | `Parity Pending` | 在 `/help` 参考命令基础上继续补更多命令语义、golden fixture 和动态来源 |
| Tools | `Parity Pending` | 在 `file_read` 参考工具基础上继续补更多工具、权限规则和 schema 样本 |
| Conversation Engine | `Parity Pending` | 在已完成 turn loop/streaming/tool continuation/budget/compact 基线上继续补更多 query 语义和真实 provider 接口 |
| TUI / REPL | `Parity Pending` | 在最小 REPL 基线上继续补 multi-line composer、history、scrollback 和更完整终端交互语义 |
| MCP | `Parity Pending` | 已完成 stdio MCP 的 `/mcp`、resource tools、动态 tool 调用与 fake stdio server 验证；后续继续补更多 transport 与通知刷新语义 |
| Skills / Plugins | `Parity Pending` | 已完成 skills 目录发现、plugin runtime 加载、动态 slash commands、plugin MCP 合并与 REPL/engine 装配；后续再补 marketplace/install/update 流水线 |
| Worktree / Session / Resume | `Parity Pending` | 在当前 JSONL/session/worktree/resume 基线上继续补更完整的 transcript 恢复、跨路径持久化与退出 worktree 交互语义 |
| Structured IO / Headless | `Parity Pending` | 已完成 `--print`、`text/json/stream-json`、stdin `stream-json` 协议、host-mediated permission request/response、interrupt 与 print-mode resume；后续继续补更多 golden fixture 与行为硬化 |
| Remote Control / Bridge | `Parity Pending` | 已完成 `remote-control` / `rc` 子命令、REPL `/remote-control` / `/rc`、bridge pointer 恢复、transport reconnect 与 fake backend/transport 验证；后续继续补真实 backend/auth 与更完整多客户端协作语义 |

## 当前重点

- 固化 `M7 / Phase 7 complete` 的运行时基线，确保 worktree/session/resume、structured IO/headless 与 remote-control/bridge 在三平台持续可验证
- 在 `DIFF-2026-001` 基线下继续扩充 bridge golden fixture、same-repo pointer recovery、transport reconnect/failure 与 remote/local busy 样本
- 进入 `Phase 8`，补齐 parity hardening、平台矩阵、golden fixture 收口与发布前检查表

## 当前差异

| 差异 ID | 说明 | 状态 |
| --- | --- | --- |
| `DIFF-2026-001` | Clawin 使用自身命名空间，不兼容 Claude 命名配置目录与主说明文件 | Accepted |
