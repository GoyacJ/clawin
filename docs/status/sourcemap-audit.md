# Sourcemap 对照审计

- 更新时间: 2026-04-09
- 上游基线: Claude Code `2.1.88`
- 参考语料: `docs/claude-code-sourcemap-main/restored-src/src/`
- 当前判定: `M7` / `Phase 8E: Release Gate Hardening`

## 审计结论

当前仓库已经完成 `Phase 1` 到 `Phase 7` 的最小主链路闭环，并为所有一级子系统建立了上游入口映射、Rust 归属、集成测试与基础 CI。但这些证据目前仍不足以把仓库正式判定为 `M8 / Phase 8 complete`：

- 一级子系统仍全部停留在 `Parity Pending`，尚未形成 `Parity Verified` 所需的完整证据包。
- `rust-ci.yml` 已升级为 Linux/macOS/Windows 固定 smoke matrix，且 2026-04-09 本地 macOS 已跑通全部 6 个固定 smoke；但当前远端只跟踪 `origin/main`，且 `origin/main` 尚无 `.github/workflows/rust-ci.yml`，因此 Linux/Windows 结果并非“待补录”，而是“当前 release baseline 尚无可追溯 CI run 可归档”。
- [release-checklist.md](/Users/goya/Repo/claude/clawin/docs/status/release-checklist.md) 已建立并成为 `M8` 门禁真源，但当前仍处于 `Not Ready for M8`。
- 多个子系统当前是“V1 最小闭环已达成”的状态，而不是“上游公开能力已充分核验”的状态。

因此，本轮审计后的正式结论是：

- `当前里程碑` 继续保持 `M7`
- `当前执行阶段` 调整为 `Phase 8E: Release Gate Hardening`
- `M8` 只在 parity hardening、平台证据和发布门禁补齐后才能切换
- `docs/status/release-checklist.md` 是当前 `M8` 门禁的唯一汇总真源

## 审计方法

- 以项目宪章为准，最高真源是 Claude Code `2.1.88` 的公开可观察行为。
- `docs/claude-code-sourcemap-main/restored-src/src/` 仅作为实现与职责边界参考，不单独构成“已完成对标”的证据。
- 对每个一级子系统统一检查四类证据：
  - 上游 sourcemap 入口与职责域
  - Clawin 当前公开行为与失败路径
  - 现有测试、golden fixture 与 smoke 覆盖
  - 当前文档是否准确表达“已实现”与“未实现”

## 一级子系统复核

| 子系统 | 上游入口 | 当前结论 | 当前证据 | Phase 8 必补 | `M8` 后扩展项 |
| --- | --- | --- | --- | --- | --- |
| Bootstrap / Entrypoint | `src/main.tsx`, `src/setup.ts` | V1 启动最小闭环已达成，但仍是 `Parity Pending` | `crates/clawin/tests/cli_smoke.rs`、`crates/clawin-bootstrap/tests/interactive_session.rs`、`crates/clawin-bootstrap/tests/non_interactive_session.rs` | 为 fast-path、no-arg、print、remote-control 补齐 sourcemap 对照样本与三平台启动证据 | 更完整 startup side effects 与上游环境探测细节 |
| Config / Settings / Persistence | `src/utils/config.ts`, `src/utils/settings/*`, `src/bootstrap/state.ts` | 配置发现、schema `1`、迁移框架已落地，但 settings 语义仍属最小实现 | `crates/clawin-config/tests/startup_config.rs` | 为配置读取/失败路径补齐更多上游样本、文档化 schema 证据、补三平台路径差异结论 | 更完整 settings merge 与更多上游字段消费 |
| Commands | `src/commands.ts`, `src/commands/*` | `/help`、`/mcp`、`/resume`、`/plugin`、`/skills` 等参考路径可用，但命令面仍明显小于上游 | `crates/clawin-commands/tests/registry.rs`、`mcp.rs`、`resume.rs`、`skills_plugins.rs` | 锁定当前 V1 命令面对应的上游入口与 golden，明确哪些未纳入 V1 | 更多本地/动态命令与完整命令生态 |
| Tools | `src/tools.ts`, `src/Tool.ts`, `src/tools/*`, `src/services/tools/*` | `file_read`、MCP resource tools、worktree tools 已形成参考实现，但工具面仍是样本集 | `crates/clawin-tools/tests/file_read.rs`、`mcp.rs`、`worktree.rs`、`permission_resolver.rs` | 为当前纳入 V1 的工具补齐 sourcemap 对照与结果 fixture 归档 | Bash/FileEdit/Grep 等更多高副作用工具 |
| Conversation Engine | `src/QueryEngine.ts`, `src/query.ts`, `src/query/*` | turn loop、streaming、tool continuation、budget、compact 已构成最小会话引擎，但并非完整 query 能力对标 | `crates/clawin-engine/tests/conversation_engine.rs` 与 fixtures | 明确当前 engine 对标的是“V1 最小可验证闭环”，并补强事件序列与失败路径证据 | 真实 provider、更多 query 语义、成本模型等扩展 |
| TUI / REPL / Screens | `src/ink/*`, `src/components/*`, `src/screens/*`, `src/keybindings/*` | no-arg REPL、slash command、cancel、resize、remote attach 已打通，但 UI 语义仍是精简版 | `crates/clawin-ui/tests/repl.rs`、`crates/clawin-platform/tests/terminal_session.rs` | 为当前 REPL 行为补齐 screenshot/snapshot 证据与三平台终端验证说明 | multi-line composer、history、scrollback、更多 screen |
| MCP | `src/services/mcp/*`, `src/tools/MCPTool/*`, `src/tools/ListMcpResourcesTool/*`, `src/tools/ReadMcpResourceTool/*` | stdio MCP 的配置、连接、动态 tool/resource 路径已达成最小闭环 | `crates/clawin-integrations/tests/mcp_manager.rs`、`fake_stdio_process.rs`、`crates/clawin-bootstrap/tests/mcp_bootstrap.rs`、`crates/clawin-tools/tests/mcp.rs` | 为 `/mcp`、resource tools、动态 tool 命名补齐上游来源样本与三平台结论 | http/sse/ws/oauth、通知驱动刷新、更多 transport |
| Skills / Plugins | `src/skills/loadSkillsDir.ts`, `src/skills/bundledSkills.ts`, `src/plugins/*`, `src/utils/markdownConfigLoader.ts` | skills/plugin runtime 加载和动态命令导出已可用，但仍是最小运行时闭环 | `crates/clawin-integrations/tests/skills_plugins.rs`、`crates/clawin-commands/tests/skills_plugins.rs`、`crates/clawin-bootstrap/tests/skills_plugins_bootstrap.rs`、`crates/clawin-commands/tests/fixtures/skills_normalized_output.txt`、`skill_command_display_output.txt`、`plugin_precedence_output.txt` | 固化显示名与 normalized token、precedence、plugin failure 的对标证据，并把 `/skills`、动态 skill command、`/plugin` 的公共文本输出锁进 fixture 基线，明确 V1 边界 | marketplace/install/update/uninstall、模板与样式生态 |
| Worktree / Session / Resume | `src/utils/worktree.ts`, `src/utils/sessionStorage.ts`, `src/utils/conversationRecovery.ts`, `src/setup.ts` | JSONL transcript、resume、same-repo 搜索、session-owned worktree 已形成关键闭环；单 session transcript 真源、restore failure path、worktree lifecycle result 和 restored active-worktree file access 已进入测试与 fixture 基线，但仍未完成 parity hardening | `crates/clawin-config/tests/session_store.rs`、`crates/clawin-config/tests/fixtures/*.jsonl`、`crates/clawin-commands/tests/resume.rs`、`crates/clawin-commands/tests/fixtures/resume_*.txt`、`crates/clawin-bootstrap/tests/resume_session.rs`、`crates/clawin-tools/tests/worktree.rs`、`crates/clawin-tools/tests/fixtures/*worktree*.json`、`crates/clawin-platform/tests/git_worktree.rs` | 保持当前 8B fixture 基线，继续补 transcript 真源、恢复失败路径、worktree 生命周期的 sourcemap 对照、三平台路径说明与 `Parity Verified` 升级证据 | 更完整 transcript 恢复、退出交互、tmux/remote 相关恢复 |
| Structured IO / Headless | `src/cli/structuredIO.ts`, `src/cli/print.ts`, `src/entrypoints/sdk/controlSchemas.ts`, `src/entrypoints/sdk/coreSchemas.ts` | `--print` 的 text/json/stream-json、host-mediated permission、busy、interrupt 与 print-mode resume/continue 已打通，并已形成首轮 fixture 基线，但仍缺完整 parity hardening 证据包 | `crates/clawin/tests/cli_smoke.rs`、`crates/clawin/tests/fixtures/print_help_text.txt`、`print_help_json.json`、`print_help_stream_json.jsonl`、`crates/clawin-bootstrap/src/print.rs` 内单元测试、`crates/clawin-bootstrap/tests/fixtures/headless_stream_text_delta.jsonl`、`headless_permission_allow.jsonl`、`headless_permission_deny.jsonl`、`headless_permission_interrupt.jsonl` | 继续补 `--print` CLI surface、structured stdin/stdout 协议与三平台 smoke 结论，作为 8C 基线保留 | 多客户端桥接与更丰富 host 集成 |
| Remote Control / Bridge | `src/remote/*`, `src/bridge/*`, `src/cli/transports/*`, `src/commands/bridge/*` | standalone `remote-control`、REPL `/remote-control`、pointer 恢复、status 输出与 reconnect 已形成最小闭环；pointer transcript anchor、permission allow/interrupt/cancel、busy、CLI unavailable/no-pointer 失败路径都已进入公共样本，但仍依赖 fake connector 证明主链路 | `crates/clawin-integrations/tests/bridge.rs`、`crates/clawin-integrations/tests/fixtures/bridge_pointer_sample.json`、`crates/clawin-bootstrap/tests/remote_control.rs`、`crates/clawin-bootstrap/tests/fixtures/remote_control_permission_allow.jsonl`、`remote_control_permission_interrupt.jsonl`、`remote_control_busy.jsonl`、`remote_control_status_connected.txt`、`remote_control_status_failed.txt`、`crates/clawin-commands/tests/remote_control.rs`、`crates/clawin-commands/tests/fixtures/remote_control_status_output.txt`、`crates/clawin/tests/cli_smoke.rs`、`crates/clawin-ui/tests/repl.rs` | 为 bridge sourcemap 对照、REPL attached 来源样本与三平台 smoke 结论补齐 `Parity Verified` 所需证据 | 真实 backend/auth、多会话 bridge、更多 transport |

## `M8` 阻塞项

在当前审计结论下，以下事项仍阻止仓库切换到 `M8`：

1. 所有一级子系统仍停留在 `Parity Pending`，没有一个被正式判定为 `Parity Verified`。
2. 三平台证据目前只完成了本地 macOS fixed smoke 归档；当前远端 release baseline 尚无 Linux/Windows fixed-smoke run，因此也就没有结果可写回 release gate。
3. 发布检查表虽已建立，但仍有 smoke matrix 与一级子系统正式判定两项总门禁未收口。
4. 多个一级子系统虽然已有测试与 fixture 基线，但仍缺 release-level sourcemap 摘录、失败路径说明或平台结论，不能直接升为 `Parity Verified`。

## Phase 8E 首批正式判定

本批首先对最接近升级的四个一级子系统做了 release-level 正式复核：

| 子系统 | 当前结论 | 已确认的 release-level 证据 | 未满足条件 |
| --- | --- | --- | --- |
| Skills / Plugins | `Parity Pending` | `src/skills/loadSkillsDir.ts`、`src/plugins/*` 的来源映射已写入审计；`crates/clawin-integrations/tests/skills_plugins.rs`、`crates/clawin-commands/tests/skills_plugins.rs`、`crates/clawin-bootstrap/tests/skills_plugins_bootstrap.rs` 与 `skills_normalized_output.txt`、`skill_command_display_output.txt`、`plugin_precedence_output.txt` 已覆盖黄金路径、失败路径和公共输出 | 当前 release baseline 缺 Linux/Windows archive，不能形成三平台正式结论 |
| Worktree / Session / Resume | `Parity Pending` | `src/utils/worktree.ts`、`src/utils/sessionStorage.ts`、`src/utils/conversationRecovery.ts` 的来源映射已写入审计；`crates/clawin-config/tests/session_store.rs`、`crates/clawin-bootstrap/tests/resume_session.rs`、`crates/clawin-commands/tests/resume.rs` 与 session/worktree fixtures 已覆盖 transcript 真源、resume failure、restore runtime 与 worktree lifecycle | 当前 release baseline 缺 Linux/Windows 路径结论与 smoke archive，不能形成三平台正式结论 |
| Structured IO / Headless | `Parity Pending` | `src/cli/structuredIO.ts`、`src/cli/print.ts` 的来源映射已写入审计；`crates/clawin/tests/cli_smoke.rs`、`crates/clawin-bootstrap/src/print.rs` 内测试与 headless fixtures 已覆盖 `--print`、stream-json、permission、busy、interrupt 与 resume-in-print | 当前 release baseline 缺 Linux/Windows smoke archive，不能形成三平台正式结论 |
| Remote Control / Bridge | `Parity Pending` | `src/remote/*`、`src/bridge/*` 的来源映射已写入审计；`crates/clawin-bootstrap/tests/remote_control.rs`、`crates/clawin-integrations/tests/bridge.rs`、`crates/clawin-ui/tests/repl.rs` 与 bridge fixtures 已覆盖 standalone/REPL attach、pointer、busy、interrupt、permission 与 reconnect/status | 当前 release baseline 缺 Linux/Windows smoke archive，不能形成三平台正式结论 |

这意味着首批正式判定已经完成，但当前并没有任何一个一级子系统满足升级到 `Parity Verified` 的必要条件。

## Phase 8E 当前判定

- 当前一级子系统仍全部维持 `Parity Pending`，没有条目因为局部 fixture 基线而自动升到 `Parity Verified`；首批四个高优先级子系统也已完成正式复核，但全部继续保持原状态。
- 当前本地质量门禁已具备 release baseline，固定 smoke 组也已在本地 macOS 跑通；但 Linux/Windows 结果仍无法归档，因为当前 remote baseline 尚无对应 workflow run。
- `M8` 是否可切换，统一由 [release-checklist.md](/Users/goya/Repo/claude/clawin/docs/status/release-checklist.md) 收口判断，而不是由单个 phase 的实现完成度决定。

## Phase 8 必须补齐

- 为一级子系统逐项补齐 `Parity Verified` 所需证据：
  - sourcemap/公开行为来源
  - 黄金路径
  - 失败路径
  - golden fixture 或等价快照
  - 三平台验证结论
- 继续维护并收口发布检查表，明确 `M8` 需要的门禁项。
- 把 `status`、`roadmap`、`parity matrix`、根 `README` 的当前阶段口径收敛为一致表达。

## `M8` 后扩展项

以下内容继续保留为后续扩展，不阻塞 `M8` 本身：

- 超出当前 V1 范围的上游长尾命令、工具与 transport
- 更完整的 UI/REPL 体验，例如 multi-line composer、复杂 scrollback、更多 screen
- 更完整的 provider、backend、marketplace、安装/更新流水线
- 未纳入 V1 首发范围的额外内部或实验能力
