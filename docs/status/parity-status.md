# 对标进度总览

- 更新时间: 2026-04-09
- 当前里程碑: `M7`
- 当前执行阶段: `Phase 8 in progress`

## 总体状态

文档与治理基线、Cargo workspace、`bootstrap/config`、`commands/tools/engine`、`TUI / REPL`、`MCP / skills / plugins`、`worktree / session / resume`、`structured IO / headless` 与 `remote control / bridge` 的最小主链路闭环均已落地，仓库当前具备 `M7` 所需的关键运行时能力。  
但按照项目宪章、对标矩阵与测试规范重新对照 `docs/claude-code-sourcemap-main/restored-src/src/` 后，当前证据仍不足以把仓库正式切到 `M8 / Phase 8 complete`：一级子系统仍全部是 `Parity Pending`，三平台证据尚未覆盖全部公开行为，且仓库内尚无发布检查表。  

本轮 sourcemap 复核的正式结论见 [sourcemap-audit.md](/Users/goya/Repo/claude/clawin/docs/status/sourcemap-audit.md)。

## 子系统状态

| 子系统 | 当前状态 | 审计结论 | Phase 8 必补 |
| --- | --- | --- | --- |
| Bootstrap / Entrypoint | `Parity Pending` | V1 启动最小闭环已达成 | 为 fast-path、no-arg、print、remote-control 补齐来源样本、golden 与三平台启动证据 |
| Config / Settings / Persistence | `Parity Pending` | schema、迁移与读取闭环可用，但仍属最小实现 | 补配置失败路径、settings 语义样本与路径平台说明 |
| Commands | `Parity Pending` | 参考命令可用，但命令面远小于上游 | 固定 V1 命令边界并为纳入命令补齐 sourcemap 与 golden |
| Tools | `Parity Pending` | 参考工具闭环可用，但仍是样本工具集 | 补当前工具的结果 fixture、失败路径与来源证据 |
| Conversation Engine | `Parity Pending` | 最小 turn loop 已达成，不等于完整 query 对标 | 补事件序列证据、失败路径与 V1 边界说明 |
| TUI / REPL | `Parity Pending` | 最小 REPL 闭环已达成，但 UI 语义仍是精简版 | 补 snapshot / 三平台终端证据与当前 UI 边界说明 |
| MCP | `Parity Pending` | stdio MCP 最小闭环已达成 | 补 `/mcp`、resource tools、动态 tool 的来源样本与平台结论 |
| Skills / Plugins | `Parity Pending` | 动态 skill/plugin runtime 已达成最小闭环 | 当前优先收口 display/token 分离、plugin precedence 与 `/skills`、`/plugin` 的公共输出证据 |
| Worktree / Session / Resume | `Parity Pending` | 关键恢复链路可用，但恢复细节仍待 hardening | 补 transcript 真源、恢复失败路径与 worktree 生命周期证据 |
| Structured IO / Headless | `Parity Pending` | `--print` 主链路已可用，但对标证据还不完整 | 补 stream-json / permission / resume 的 golden 与平台说明 |
| Remote Control / Bridge | `Parity Pending` | bridge 最小闭环已达成，但仍依赖 fake connector 验证 | 补 pointer recovery、busy/cancel/reconnect 的来源样本与 fixture |

## 当前重点

- 固化 `M7` 已实现的最小闭环，不再把“最小实现”直接表述为“已完成对标”
- `Phase 8A` 当前优先批次先收口 `Skills / Plugins`，把 display/token、precedence 与 plugin failure 文本输出锁进测试与 fixture
- 以 [sourcemap-audit.md](/Users/goya/Repo/claude/clawin/docs/status/sourcemap-audit.md) 为基线，逐子系统补齐 `Parity Verified` 所需证据
- 新增发布检查表，并在 `status`、`roadmap`、`parity matrix` 与根入口文档之间收敛里程碑口径

## 当前差异

| 差异 ID | 说明 | 状态 |
| --- | --- | --- |
| `DIFF-2026-001` | Clawin 使用自身命名空间，不兼容 Claude 命名配置目录与主说明文件 | Accepted |
