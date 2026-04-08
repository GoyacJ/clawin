# 对标进度总览

- 更新时间: 2026-04-08
- 当前阶段: `M1`

## 总体状态

文档与治理基线、Cargo workspace、最小 `clawin` 可执行骨架和基础 CI 已建立。仓库已进入 `M1`，并具备继续推进 `bootstrap/config` 首轮迁移的装配基础。

## 子系统状态

| 子系统 | 当前状态 | 下一步 |
| --- | --- | --- |
| Bootstrap / Entrypoint | `In Progress` | 以最小 CLI stub 为基线继续扩展启动链路与配置装配 |
| Config / Settings / Persistence | `Spec Ready` | 作为首轮试运行对象，先冻结 schema 与 migration |
| Commands | `Spec Ready` | 设计 command registry |
| Tools | `Spec Ready` | 设计 tool trait、权限和注册表 |
| Conversation Engine | `Spec Ready` | 在 commands/tools 骨架后推进 |
| TUI / REPL | `Spec Ready` | 等 engine 事件模型稳定后开始 |
| MCP | `Spec Ready` | 在 engine 主链路稳定后推进 |
| Skills / Plugins | `Spec Ready` | 在 config 与 loader 规则落地后推进 |
| Worktree / Session / Resume | `Spec Ready` | 在 config 与 engine 有基本实现后推进 |
| Remote / Structured IO | `Not Started` | 需在后续阶段补细化边界 |

## 当前重点

- 固化 `M1` workspace、crate 边界和基础 CI
- 推进 `bootstrap/config` 首轮能力实现
- 补充 CLI help / startup 行为样本与 fixture

## 当前差异

| 差异 ID | 说明 | 状态 |
| --- | --- | --- |
| `DIFF-2026-001` | Clawin 使用自身命名空间，不兼容 Claude 命名配置目录与主说明文件 | Accepted |
