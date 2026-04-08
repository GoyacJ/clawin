# 对标进度总览

- 更新时间: 2026-04-08
- 当前阶段: `M3`

## 总体状态

文档与治理基线、Cargo workspace、最小 `clawin` 可执行骨架、`bootstrap/config` 首轮闭环，以及 `commands/tools/engine` 的 `M3` 最小闭环均已落地。当前仓库已具备继续推进 turn loop、更多命令/工具样本和更完整行为对标的稳定基线。

## 子系统状态

| 子系统 | 当前状态 | 下一步 |
| --- | --- | --- |
| Bootstrap / Entrypoint | `Parity Pending` | 在现有 startup/config 装配之上补充 commands/tools 入口与更多行为 fixture |
| Config / Settings / Persistence | `Parity Pending` | 在现有 schema/migration 闭环之上继续补 settings 合并语义与更多对标样本 |
| Commands | `Parity Pending` | 在 `/help` 参考命令基础上继续补更多命令语义、golden fixture 和动态来源 |
| Tools | `Parity Pending` | 在 `file_read` 参考工具基础上继续补更多工具、权限规则和 schema 样本 |
| Conversation Engine | `In Progress` | 从最小 session runner 继续推进 turn loop、事件流和更完整会话状态 |
| TUI / REPL | `Spec Ready` | 等 engine 事件模型稳定后开始 |
| MCP | `Spec Ready` | 在 engine 主链路稳定后推进 |
| Skills / Plugins | `Spec Ready` | 在 config 与 loader 规则落地后推进 |
| Worktree / Session / Resume | `Spec Ready` | 在 config 与 engine 有基本实现后推进 |
| Remote / Structured IO | `Not Started` | 需在后续阶段补细化边界 |

## 当前重点

- 固化 `M3` 的最小 commands/tools/engine 闭环
- 推进 engine turn loop 与更多命令/工具对标样本
- 在 `DIFF-2026-001` 基线下继续补充 CLI、tool result 和 session event fixture

## 当前差异

| 差异 ID | 说明 | 状态 |
| --- | --- | --- |
| `DIFF-2026-001` | Clawin 使用自身命名空间，不兼容 Claude 命名配置目录与主说明文件 | Accepted |
