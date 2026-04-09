# 发布检查表

- 更新时间: 2026-04-09
- 当前发布判定: `Not Ready for M8`
- 当前执行阶段: `Phase 8E: Release Gate Hardening`

## 总门禁

| 门禁 | 当前状态 | 当前证据 | `M8` 要求 |
| --- | --- | --- | --- |
| 本地质量门禁 | `Done` | 已执行 `cargo fmt --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace`、`cargo build --workspace` | 合并前保持绿灯 |
| 文档口径一致性 | `Done` | `README`、`parity-status`、`sourcemap-audit`、`parity-matrix`、`master-roadmap`、本检查表已统一到 `Phase 8E` | 不得再出现阶段漂移 |
| 三平台 smoke matrix | `Pending` | `rust-ci.yml` 已在当前本地基线升级为 Linux/macOS/Windows 固定 smoke 组；2026-04-09 本地 macOS 已跑通全部 6 个固定 smoke；但 `origin` 仍只跟踪 `main`，且 `origin/main` 尚无 `.github/workflows/rust-ci.yml`，因此当前 release baseline 还没有可归档的 Linux/Windows CI run | 三平台 smoke 全部通过，且结果可追溯 |
| 一级子系统正式判定 | `Pending` | 首批正式判定已覆盖 `Skills / Plugins`、`Worktree / Session / Resume`、`Structured IO / Headless`、`Remote Control / Bridge` 四个高优先级子系统，但它们都因缺 Linux/Windows release archive 继续保持 `Parity Pending` | 全部一级子系统达到 `Parity Verified` 或 `Accepted Difference` |

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
| Linux | `Pending` | 已确认 `origin/main` 尚无 `.github/workflows/rust-ci.yml`，当前 `develop` release baseline 只存在本地提交，因此 GitHub 上没有对应的 Linux fixed-smoke run 可供归档 | 需先把包含 `rust-ci.yml` 的 release baseline push/merge 到远端，再归档 Linux 结果 |
| Windows | `Pending` | 已确认 `origin/main` 尚无 `.github/workflows/rust-ci.yml`，当前 `develop` release baseline 只存在本地提交，因此 GitHub 上没有对应的 Windows fixed-smoke run 可供归档 | 需先把包含 `rust-ci.yml` 的 release baseline push/merge 到远端，再归档 Windows 结果 |

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
| Skills / Plugins | `Parity Pending` | 上游入口已固定到 `src/skills/loadSkillsDir.ts`、`src/plugins/*`；黄金路径/失败路径与公共输出已由 `crates/clawin-integrations/tests/skills_plugins.rs`、`crates/clawin-commands/tests/skills_plugins.rs`、`crates/clawin-bootstrap/tests/skills_plugins_bootstrap.rs` 以及 `skills_normalized_output.txt`、`skill_command_display_output.txt`、`plugin_precedence_output.txt` 锁定 | 首批正式复核已完成，但当前 release baseline 缺 Linux/Windows archive，因此还不能升为 `Parity Verified` |
| Worktree / Session / Resume | `Parity Pending` | 上游入口已固定到 `src/utils/worktree.ts`、`src/utils/sessionStorage.ts`；黄金路径/失败路径与 fixture 已由 `crates/clawin-config/tests/session_store.rs`、`crates/clawin-bootstrap/tests/resume_session.rs`、`crates/clawin-commands/tests/resume.rs`、resume/worktree fixtures 锁定 | 首批正式复核已完成，但当前 release baseline 缺 Linux/Windows 路径结论与 smoke archive，因此还不能升为 `Parity Verified` |
| Structured IO / Headless | `Parity Pending` | 上游入口已固定到 `src/cli/structuredIO.ts`、`src/cli/print.ts`；`--print` CLI、permission、busy/interrupt 与 resume-in-print 证据已由 `crates/clawin/tests/cli_smoke.rs`、`crates/clawin-bootstrap/src/print.rs` 内测试及 headless fixtures 锁定 | 首批正式复核已完成，但当前 release baseline 缺 Linux/Windows smoke archive，因此还不能升为 `Parity Verified` |
| Remote Control / Bridge | `Parity Pending` | 上游入口已固定到 `src/remote/*`、`src/bridge/*`；standalone/REPL attached、pointer、busy/interrupt、permission 与 status 输出证据已由 `crates/clawin-bootstrap/tests/remote_control.rs`、`crates/clawin-integrations/tests/bridge.rs`、`crates/clawin-ui/tests/repl.rs` 与 bridge fixtures 锁定 | 首批正式复核已完成，但当前 release baseline 缺 Linux/Windows smoke archive，因此还不能升为 `Parity Verified` |

## 当前结论

- `DIFF-2026-001` 继续是当前唯一 accepted difference。
- 当前批次已经完成固定 smoke 的 macOS 本地归档，并完成首批四个高优先级一级子系统的正式复核。
- Linux/Windows 仍不是“待补录的现成结果”，而是“当前远端 release baseline 尚无对应 fixed-smoke run 可归档”。
- 当前仓库可以继续保持 `M7` 并推进 `Phase 8E`，但还不能切到 `M8`。
- 只有当三平台 smoke 结果、一级子系统正式判定和本检查表全部收口后，才允许把里程碑切到 `M8 / Phase 8 complete`。
