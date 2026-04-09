# Sourcemap 对照审计

- 更新时间: 2026-04-09
- 上游基线: Claude Code `2.1.88`
- 参考语料: `docs/claude-code-sourcemap-main/restored-src/src/`
- 当前判定: `M7` / `Phase 8 in progress`

## 审计结论

当前仓库已经完成 `Phase 1` 到 `Phase 7` 的最小主链路闭环，并为所有一级子系统建立了上游入口映射、Rust 归属、集成测试与基础 CI。但这些证据目前仍不足以把仓库正式判定为 `M8 / Phase 8 complete`：

- 一级子系统仍全部停留在 `Parity Pending`，尚未形成 `Parity Verified` 所需的完整证据包。
- 现有 CI 只覆盖 `cargo fmt/clippy/test/build` 与三平台 `cargo build + clawin --help` smoke，尚不足以证明所有公开行为都已完成三平台对标验证。
- 仓库内尚无发布检查表，因而不能宣称“发布准备完成”。
- 多个子系统当前是“V1 最小闭环已达成”的状态，而不是“上游公开能力已充分核验”的状态。

因此，本轮审计后的正式结论是：

- `当前里程碑` 继续保持 `M7`
- `当前执行阶段` 调整为 `Phase 8 in progress`
- `M8` 只在 parity hardening、平台证据和发布门禁补齐后才能切换

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
| Worktree / Session / Resume | `src/utils/worktree.ts`, `src/utils/sessionStorage.ts`, `src/utils/conversationRecovery.ts`, `src/setup.ts` | JSONL transcript、resume、same-repo 搜索、session-owned worktree 已形成关键闭环，但恢复细节仍未完全 harden | `crates/clawin-config/tests/session_store.rs`、`crates/clawin-bootstrap/tests/resume_session.rs`、`crates/clawin-tools/tests/worktree.rs`、`crates/clawin-platform/tests/git_worktree.rs` | 为 transcript 真源、恢复失败路径、worktree 生命周期补齐 sourcemap 对照和平台说明 | 更完整 transcript 恢复、退出交互、tmux/remote 相关恢复 |
| Structured IO / Headless | `src/cli/structuredIO.ts`, `src/cli/print.ts`, `src/entrypoints/sdk/controlSchemas.ts`, `src/entrypoints/sdk/coreSchemas.ts` | `--print` 的 text/json/stream-json 与 host-mediated permission 已打通，但仍缺系统化对标证据 | `crates/clawin/tests/cli_smoke.rs`、`crates/clawin-bootstrap/src/print.rs` 内单元测试 | 为 `--print` 输入输出协议补齐 golden、来源样本与三平台 smoke 结论 | 多客户端桥接与更丰富 host 集成 |
| Remote Control / Bridge | `src/remote/*`, `src/bridge/*`, `src/cli/transports/*`, `src/commands/bridge/*` | standalone `remote-control`、REPL `/remote-control`、pointer 恢复与 reconnect 已形成最小闭环，但仍依赖 fake connector 证明主链路 | `crates/clawin-integrations/tests/bridge.rs`、`crates/clawin-bootstrap/tests/remote_control.rs`、`crates/clawin-ui/tests/repl.rs` | 为 bridge 协议、pointer 恢复、busy/cancel/reconnect 补齐 sourcemap 对照与可追溯 fixture | 真实 backend/auth、多会话 bridge、更多 transport |

## `M8` 阻塞项

在当前审计结论下，以下事项仍阻止仓库切换到 `M8`：

1. 所有一级子系统仍停留在 `Parity Pending`，没有一个被正式判定为 `Parity Verified`。
2. 缺少覆盖所有一级子系统的三平台证据说明；现有 CI 只证明构建和 `--help` smoke。
3. 缺少发布检查表，因此不能宣称“发布准备完成”。
4. 根入口文档此前仍有阶段口径漂移，需要本轮审计一起收口。

## Phase 8 必须补齐

- 为一级子系统逐项补齐 `Parity Verified` 所需证据：
  - sourcemap/公开行为来源
  - 黄金路径
  - 失败路径
  - golden fixture 或等价快照
  - 三平台验证结论
- 新增并维护发布检查表，明确 `M8` 需要的门禁项。
- 把 `status`、`roadmap`、`parity matrix`、根 `README` 的当前阶段口径收敛为一致表达。

## `M8` 后扩展项

以下内容继续保留为后续扩展，不阻塞 `M8` 本身：

- 超出当前 V1 范围的上游长尾命令、工具与 transport
- 更完整的 UI/REPL 体验，例如 multi-line composer、复杂 scrollback、更多 screen
- 更完整的 provider、backend、marketplace、安装/更新流水线
- 未纳入 V1 首发范围的额外内部或实验能力
