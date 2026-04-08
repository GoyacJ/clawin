# 对标进度总览

- 更新时间: 2026-04-08
- 当前阶段: `M2`

## 总体状态

文档与治理基线、Cargo workspace、最小 `clawin` 可执行骨架和基础 CI 已建立，`bootstrap/config` 首轮闭环也已落地。仓库已进入 `M2`，具备继续推进 commands/tools 基础设施的稳定装配面。

## 子系统状态

| 子系统 | 当前状态 | 下一步 |
| --- | --- | --- |
| Bootstrap / Entrypoint | `Parity Pending` | 在现有 startup/config 装配之上补充 commands/tools 入口与更多行为 fixture |
| Config / Settings / Persistence | `Parity Pending` | 在现有 schema/migration 闭环之上继续补 settings 合并语义与更多对标样本 |
| Commands | `Spec Ready` | 设计 command registry |
| Tools | `Spec Ready` | 设计 tool trait、权限和注册表 |
| Conversation Engine | `Spec Ready` | 在 commands/tools 骨架后推进 |
| TUI / REPL | `Spec Ready` | 等 engine 事件模型稳定后开始 |
| MCP | `Spec Ready` | 在 engine 主链路稳定后推进 |
| Skills / Plugins | `Spec Ready` | 在 config 与 loader 规则落地后推进 |
| Worktree / Session / Resume | `Spec Ready` | 在 config 与 engine 有基本实现后推进 |
| Remote / Structured IO | `Not Started` | 需在后续阶段补细化边界 |

## 当前重点

- 固化 `M2` 的 bootstrap/config 闭环
- 推进 commands/tools 首轮基础设施
- 继续补充 CLI startup/config 行为样本与 fixture

## 当前差异

| 差异 ID | 说明 | 状态 |
| --- | --- | --- |
| `DIFF-2026-001` | Clawin 使用自身命名空间，不兼容 Claude 命名配置目录与主说明文件 | Accepted |
