# 对标进度总览

- 更新时间: 2026-04-09
- 当前里程碑: `M7`
- 当前执行阶段: `Phase 8E: Release Gate Hardening`

## 总体状态

文档与治理基线、Cargo workspace、`bootstrap/config`、`commands/tools/engine`、`TUI / REPL`、`MCP / skills / plugins`、`worktree / session / resume`、`structured IO / headless` 与 `remote control / bridge` 的最小主链路闭环均已落地，仓库当前具备 `M7` 所需的关键运行时能力。  
但按照项目宪章、对标矩阵与测试规范重新对照 `docs/claude-code-sourcemap-main/restored-src/src/` 后，当前证据仍不足以把仓库正式切到 `M8 / Phase 8 complete`：GitHub Actions run [24181239142](https://github.com/GoyacJ/clawin/actions/runs/24181239142) 已完成三平台固定 smoke 归档，且首批四个高优先级一级子系统已经升级为 `Parity Verified`，但其余一级子系统仍待正式判定，发布检查表尚未全部完成。

本轮 sourcemap 复核的正式结论见 [sourcemap-audit.md](/Users/goya/Repo/claude/clawin/docs/status/sourcemap-audit.md)。
当前 `M8` 门禁真源见 [release-checklist.md](/Users/goya/Repo/claude/clawin/docs/status/release-checklist.md)。

## 子系统状态

| 子系统 | 当前状态 | 审计结论 | Phase 8 必补 |
| --- | --- | --- | --- |
| Bootstrap / Entrypoint | `Parity Pending` | V1 启动最小闭环已达成 | 为 fast-path、no-arg、print、remote-control 补齐来源样本、golden 与 release-level failure-path 归档 |
| Config / Settings / Persistence | `Parity Pending` | schema、迁移与读取闭环可用，但仍属最小实现 | 补配置失败路径、settings 语义样本与路径平台说明 |
| Commands | `Parity Pending` | 参考命令可用，但命令面远小于上游 | 固定 V1 命令边界并为纳入命令补齐 sourcemap 与 golden |
| Tools | `Parity Pending` | 参考工具闭环可用，但仍是样本工具集 | 补当前工具的结果 fixture、失败路径与来源证据 |
| Conversation Engine | `Parity Pending` | 最小 turn loop 已达成，不等于完整 query 对标 | 补事件序列证据、失败路径与 V1 边界说明 |
| TUI / REPL | `Parity Pending` | 最小 REPL 闭环已达成，但 UI 语义仍是精简版 | 补 snapshot / 三平台终端证据与当前 UI 边界说明 |
| MCP | `Parity Pending` | stdio MCP 最小闭环已达成 | 补 `/mcp`、resource tools、动态 tool 的来源样本与平台结论 |
| Skills / Plugins | `Parity Verified` | display/token 分离、plugin precedence、`/skills`、`/plugin` 与动态命令导出已具备来源映射、fixture、失败路径与三平台 smoke 结论 | 当前不再单独阻塞 `M8`；保持 release-level 证据基线 |
| Worktree / Session / Resume | `Parity Verified` | transcript 真源、restore failure path、worktree 生命周期与 restored runtime/file-read 已具备来源映射、fixture、失败路径与三平台 smoke 结论 | 当前不再单独阻塞 `M8`；保持 release-level 证据基线 |
| Structured IO / Headless | `Parity Verified` | `--print` 主链路、stream-json 协议、permission、busy、interrupt、resume-in-print 已具备来源映射、fixture 与三平台 smoke 结论 | 当前不再单独阻塞 `M8`；保持 release-level 证据基线 |
| Remote Control / Bridge | `Parity Verified` | pointer、busy/cancel/reconnect、status 输出、standalone/REPL attach 与 fake connector acceptance 已具备来源映射、fixture、失败路径与三平台 smoke 结论 | 当前不再单独阻塞 `M8`；保持 release-level 证据基线 |

## 当前重点

- 固化 `M7` 已实现的最小闭环，不再把“最小实现”直接表述为“已完成对标”
- `Phase 8E` 当前固定为 release gate hardening，不再开新功能阶段
- 固定 smoke 组已由 GitHub Actions run [24181239142](https://github.com/GoyacJ/clawin/actions/runs/24181239142) 在 Linux/macOS/Windows 完成可追溯归档
- 已完成 `Skills / Plugins`、`Worktree / Session / Resume`、`Structured IO / Headless`、`Remote Control / Bridge` 的首批正式发布复核，并已升级为 `Parity Verified`
- 当前剩余阻塞集中在 `Bootstrap / Entrypoint`、`Config / Settings / Persistence`、`Commands`、`Tools`、`Conversation Engine`、`TUI / REPL`、`MCP` 七个一级子系统的正式判定
- 在 release checklist 全部收口前，仓库继续保持 `M7`

## 当前差异

| 差异 ID | 说明 | 状态 |
| --- | --- | --- |
| `DIFF-2026-001` | Clawin 使用自身命名空间，不兼容 Claude 命名配置目录与主说明文件 | Accepted |
