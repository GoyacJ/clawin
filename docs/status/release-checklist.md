# 发布检查表

- 更新时间: 2026-04-09
- 当前发布判定: `Not Ready for M8`
- 当前执行阶段: `Phase 8E: Release Gate Hardening`

## 总门禁

| 门禁 | 当前状态 | 当前证据 | `M8` 要求 |
| --- | --- | --- | --- |
| 本地质量门禁 | `Done` | 已执行 `cargo fmt --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace`、`cargo build --workspace` | 合并前保持绿灯 |
| 文档口径一致性 | `Done` | `parity-status`、`sourcemap-audit`、`parity-matrix`、`master-roadmap`、本检查表已统一到 `Phase 8E` | 不得再出现阶段漂移 |
| 三平台 smoke matrix | `Pending` | `rust-ci.yml` 已升级为 Linux/macOS/Windows 固定 smoke 组 | 三平台 smoke 全部通过，且结果可追溯 |
| 一级子系统正式判定 | `Pending` | 对标矩阵与 sourcemap 审计已具备逐项判定框架 | 全部一级子系统达到 `Parity Verified` 或 `Accepted Difference` |

## 三平台固定 Smoke 组

以下命令是 `M8` 前固定的三平台行为级 smoke，Linux、macOS、Windows 都必须跑通：

1. `cargo test -p clawin --test cli_smoke`
2. `cargo test -p clawin-config --test session_store`
3. `cargo test -p clawin-bootstrap --test resume_session`
4. `cargo test -p clawin-bootstrap --test remote_control`
5. `cargo test -p clawin-ui --test repl`
6. `cargo test -p clawin-integrations --test bridge`

## 一级子系统正式判定

| 子系统 | 当前正式结论 | 当前发布阻塞 |
| --- | --- | --- |
| Bootstrap / Entrypoint | `Parity Pending` | 缺三平台 startup smoke 结果与 release gate 归档 |
| Config / Settings / Persistence | `Parity Pending` | 缺 settings 行为样本与三平台路径结论 |
| Commands | `Parity Pending` | 缺 V1 命令面来源摘录与 release-level golden 归档 |
| Tools | `Parity Pending` | 缺当前纳入 V1 的工具结果样本与失败路径审计归档 |
| Conversation Engine | `Parity Pending` | 缺 turn loop / failure path 的 release-level 证据包 |
| TUI / REPL / Screens | `Parity Pending` | 缺 snapshot / 三平台终端结论 |
| MCP | `Parity Pending` | 缺 `/mcp`、resource tools、动态 tool 的三平台结论 |
| Skills / Plugins | `Parity Pending` | 缺 sourcemap 摘录、三平台结论与正式 `Parity Verified` 升级证据 |
| Worktree / Session / Resume | `Parity Pending` | 缺 sourcemap 摘录、三平台路径结论与正式 `Parity Verified` 升级证据 |
| Structured IO / Headless | `Parity Pending` | 缺 `--print` 协议来源摘录与三平台 smoke 结果 |
| Remote Control / Bridge | `Parity Pending` | 缺 REPL attached 来源样本与三平台 smoke 结果 |

## 当前结论

- `DIFF-2026-001` 继续是当前唯一 accepted difference。
- 当前仓库可以继续保持 `M7` 并推进 `Phase 8E`，但还不能切到 `M8`。
- 只有当三平台 smoke 结果、一级子系统正式判定和本检查表全部收口后，才允许把里程碑切到 `M8 / Phase 8 complete`。
