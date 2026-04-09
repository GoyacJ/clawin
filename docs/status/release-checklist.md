# 发布检查表

- 更新时间: 2026-04-09
- 当前发布判定: `Not Ready for M8`
- 当前执行阶段: `Phase 8E: Release Gate Hardening`

## 总门禁

| 门禁 | 当前状态 | 当前证据 | `M8` 要求 |
| --- | --- | --- | --- |
| 本地质量门禁 | `Done` | 已执行 `cargo fmt --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace`、`cargo build --workspace` | 合并前保持绿灯 |
| 文档口径一致性 | `Done` | `README`、`parity-status`、`sourcemap-audit`、`parity-matrix`、`master-roadmap`、本检查表已统一到 `Phase 8E` | 不得再出现阶段漂移 |
| 三平台 smoke matrix | `Pending` | `rust-ci.yml` 已升级为 Linux/macOS/Windows 固定 smoke 组；2026-04-09 本地 macOS 已跑通全部 6 个固定 smoke | 三平台 smoke 全部通过，且结果可追溯 |
| 一级子系统正式判定 | `Pending` | 本检查表与 sourcemap 审计已收口一级子系统正式结论，但仍全部维持 `Parity Pending` | 全部一级子系统达到 `Parity Verified` 或 `Accepted Difference` |

## 三平台固定 Smoke 组

以下命令是 `M8` 前固定的三平台行为级 smoke，Linux、macOS、Windows 都必须跑通：

1. `cargo test -p clawin --test cli_smoke`
2. `cargo test -p clawin-config --test session_store`
3. `cargo test -p clawin-bootstrap --test resume_session`
4. `cargo test -p clawin-bootstrap --test remote_control`
5. `cargo test -p clawin-ui --test repl`
6. `cargo test -p clawin-integrations --test bridge`

## 当前批次 Smoke 归档

| 平台 | 当前结果 | 当前证据 | 当前阻塞 |
| --- | --- | --- | --- |
| macOS | `Done` | 2026-04-09 本地已跑通 `cli_smoke`、`session_store`、`resume_session`、`remote_control`、`repl`、`bridge` 六个固定 smoke | 结果已进入本检查表，但还不能替代 Linux/Windows 结论 |
| Linux | `Pending` | `rust-ci.yml` 已固定 smoke matrix | 待 CI run 结果归档到 release gate 证据 |
| Windows | `Pending` | `rust-ci.yml` 已固定 smoke matrix | 待 CI run 结果归档到 release gate 证据 |

## 一级子系统正式判定

| 子系统 | 当前正式结论 | 当前证据摘要 | 当前发布阻塞 |
| --- | --- | --- | --- |
| Bootstrap / Entrypoint | `Parity Pending` | `src/main.tsx` / `src/setup.ts`；`crates/clawin/tests/cli_smoke.rs`；`crates/clawin-bootstrap/tests/interactive_session.rs`；`crates/clawin-bootstrap/tests/non_interactive_session.rs` | 缺 Linux/Windows startup smoke 结果与 release-level 来源摘录 |
| Config / Settings / Persistence | `Parity Pending` | `src/utils/config.ts`、`src/utils/settings/*`；`crates/clawin-config/tests/startup_config.rs` | 缺 settings 行为样本、失败路径归档与三平台路径结论 |
| Commands | `Parity Pending` | `src/commands.ts`；`crates/clawin-commands/tests/registry.rs`、`mcp.rs`、`resume.rs`、`skills_plugins.rs` | 缺 V1 命令面来源摘录与 release-level golden 归档 |
| Tools | `Parity Pending` | `src/tools.ts`；`crates/clawin-tools/tests/file_read.rs`、`mcp.rs`、`worktree.rs`、`permission_resolver.rs` | 缺当前纳入 V1 的工具结果样本与失败路径审计归档 |
| Conversation Engine | `Parity Pending` | `src/QueryEngine.ts`、`src/query.ts`；`crates/clawin-engine/tests/conversation_engine.rs` 与 fixtures | 缺 turn loop / failure path 的 release-level 证据包 |
| TUI / REPL / Screens | `Parity Pending` | `src/ink/*`、`src/components/*`、`src/screens/*`；`crates/clawin-ui/tests/repl.rs`、`crates/clawin-platform/tests/terminal_session.rs` | 缺 snapshot / 三平台终端结论 |
| MCP | `Parity Pending` | `src/services/mcp/*`；`crates/clawin-integrations/tests/mcp_manager.rs`、`crates/clawin-bootstrap/tests/mcp_bootstrap.rs`、`crates/clawin-tools/tests/mcp.rs` | 缺 `/mcp`、resource tools、动态 tool 的三平台结论 |
| Skills / Plugins | `Parity Pending` | `src/skills/loadSkillsDir.ts`、`src/plugins/*`；`crates/clawin-integrations/tests/skills_plugins.rs`、`crates/clawin-commands/tests/skills_plugins.rs`、skills/plugin fixtures | 缺 sourcemap 摘录、Linux/Windows 结论与正式 `Parity Verified` 升级证据 |
| Worktree / Session / Resume | `Parity Pending` | `src/utils/worktree.ts`、`src/utils/sessionStorage.ts`；`crates/clawin-config/tests/session_store.rs`、`crates/clawin-bootstrap/tests/resume_session.rs`、resume/worktree fixtures | 缺 sourcemap 摘录、Linux/Windows 路径结论与正式 `Parity Verified` 升级证据 |
| Structured IO / Headless | `Parity Pending` | `src/cli/structuredIO.ts`、`src/cli/print.ts`；`crates/clawin/tests/cli_smoke.rs`、`crates/clawin-bootstrap/src/print.rs` 内测试、headless fixtures | 缺 `--print` 协议来源摘录与 Linux/Windows smoke 结果 |
| Remote Control / Bridge | `Parity Pending` | `src/remote/*`、`src/bridge/*`；`crates/clawin-bootstrap/tests/remote_control.rs`、`crates/clawin-integrations/tests/bridge.rs`、`crates/clawin-ui/tests/repl.rs` 与 bridge fixtures | 缺 REPL attached 来源样本与 Linux/Windows smoke 结果 |

## 当前结论

- `DIFF-2026-001` 继续是当前唯一 accepted difference。
- 当前批次已经完成固定 smoke 的 macOS 本地归档，并把一级子系统正式判定收口到 release gate 文档。
- 当前仓库可以继续保持 `M7` 并推进 `Phase 8E`，但还不能切到 `M8`。
- 只有当三平台 smoke 结果、一级子系统正式判定和本检查表全部收口后，才允许把里程碑切到 `M8 / Phase 8 complete`。
