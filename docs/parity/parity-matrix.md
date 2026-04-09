# Clawin 对标矩阵

- 基线版本: Claude Code `2.1.88`
- 基线来源: `docs/claude-code-sourcemap-main/restored-src/src/`
- 维护原则: 新子系统开工前必须先补齐本表对应条目

## 状态说明

| 状态 | 含义 |
| --- | --- |
| `Not Started` | 尚未形成足够实施基线 |
| `Spec Ready` | 已有范围、责任边界与初步验收标准，可进入实现 |
| `In Progress` | 已开始 Rust 迁移与实现 |
| `Parity Pending` | 功能已基本完成，等待系统性对标与平台验证 |
| `Parity Verified` | 行为、测试、平台验证均通过 |
| `Accepted Difference` | 允许偏离上游，且已完成 ADR/差异登记 |

## 一级子系统矩阵

| 子系统 | 上游 TS 入口 | Rust 归属 | 状态 | 验收测试 | 差异说明 | 风险 |
| --- | --- | --- | --- | --- | --- | --- |
| Bootstrap / Entrypoint | `src/main.tsx`, `src/setup.ts` | `clawin-bootstrap` | `Parity Pending` | `clawin --help/--version` fast-path、无参数进入 bootstrap、`SessionRuntime` 首轮装配、commands/tools/engine 最小装配、invalid-config 失败路径、三平台 startup smoke | 暂无 | 启动链路后续仍会接入更完整 engine，需防止 side effects 过早渗透 |
| Config / Settings / Persistence | `src/utils/config.ts`, `src/utils/settings/*`, `src/bootstrap/state.ts` | `clawin-config` + `clawin-bootstrap` | `Parity Pending` | `project_key` 归一化、`~/.clawin/config.json` 初始化、`~/.clawin/settings.json`/`.clawin/settings.json` 发现、schema `1`、migration 备份、invalid settings/config 失败路径 | `DIFF-2026-001` | settings 合并语义和更多上游字段仍待继续细化 |
| Commands | `src/commands.ts`, `src/commands/*` | `clawin-commands` | `Parity Pending` | command registry、name/alias 解析、lazy load、`/help` reference command、unknown-command 失败路径、golden fixture | `DIFF-2026-001` | 命令分 prompt 型与本地型，当前仅完成最小 local 路径 |
| Tools | `src/tools.ts`, `src/Tool.ts`, `src/tools/*`, `src/services/tools/*` | `clawin-tools` | `Parity Pending` | tool schema、permission decision、`file_read` reference tool、tool result golden fixture、invalid-input/unsupported-file 失败路径 | `DIFF-2026-001` | 工具是行为对标核心，高耦合 permissions 和 engine，当前仅覆盖最小 read-only 样本 |
| Conversation Engine | `src/QueryEngine.ts`, `src/query.ts`, `src/query/*` | `clawin-engine` | `Parity Pending` | transcript state、`submit_message` turn loop、typed streaming events、`/help` fast-path、`file_read` tool continuation、token budget continuation、deterministic compaction、cancel path、fake-driver end-to-end acceptance | `DIFF-2026-001` | 真实 provider/API client、更多 query deps 和完整 compact 语义仍待后续阶段 |
| TUI / REPL / Screens | `src/ink/*`, `src/components/*`, `src/screens/*`, `src/keybindings/*` | `clawin-ui` + `clawin-platform` | `Parity Pending` | interactive no-arg 路由、REPL slash command、unavailable-driver prompt path、ctrl-c cancel/exit、resize、TUI snapshot acceptance | 允许 Rust TUI 技术栈重写实现，行为不变 | 三平台终端能力和渲染细节差异大，当前仍未覆盖完整 scrollback/history |
| MCP | `src/services/mcp/*`, `src/tools/MCPTool/*`, `src/tools/ListMcpResourcesTool/*`, `src/tools/ReadMcpResourceTool/*` | `clawin-integrations` + `clawin-tools` + `clawin-bootstrap` + `clawin-commands` | `Parity Pending` | `mcpServers` merge、stdio connect/init、`/mcp list`、`/mcp reload`、MCP dynamic tool naming、`list_mcp_resources`、`read_mcp_resource`、bootstrap + engine + REPL orchestration | `DIFF-2026-001` | 当前仅覆盖 stdio transport；http/sse/ws/oauth、通知驱动刷新与 prompts/skills/plugins 仍未进入本阶段 |
| Skills / Plugins | `src/skills/loadSkillsDir.ts`, `src/skills/bundledSkills.ts`, `src/plugins/*`, `src/utils/markdownConfigLoader.ts` | `clawin-integrations` + `clawin-commands` + `clawin-bootstrap` | `Parity Pending` | skills 目录发现、frontmatter parsing、project/user 覆盖、dynamic skill command export、display name 与 normalized slash-command token 分离、plugin manifest loading、plugin-contributed command loading、plugin-contributed MCP merge、`/skills`、`/plugin`、non-blocking plugin failure path、bootstrap + engine + REPL orchestration | `DIFF-2026-001` | 当前仅覆盖本地运行时加载闭环；marketplace/install/update/uninstall、templates/workflows/output styles 仍待后续阶段 |
| Worktree / Session / Resume | `src/utils/worktree.ts`, `src/utils/sessionStorage.ts`, `src/utils/conversationRecovery.ts`, `src/setup.ts` | `clawin-bootstrap` + `clawin-config` + `clawin-core` + `clawin-engine` + `clawin-tools` + `clawin-ui` + `clawin-platform` | `Parity Pending` | session path layout、JSONL transcript、单 session transcript 真源在 worktree 生命周期内保持稳定、`/resume` / `/continue`、CLI `--continue` / `--resume`、same-repo worktree search、`EnterWorktree` / `ExitWorktree`、restore failure path、restored runtime orchestration、restore 后 active worktree 相对路径工具解析 | `DIFF-2026-001` | 核心恢复链路已进入 fixture 基线，但 sourcemap 来源摘录、三平台路径说明与更完整退出交互仍待继续 harden |
| Structured IO / Headless | `src/cli/structuredIO.ts`, `src/cli/print.ts`, `src/entrypoints/sdk/controlSchemas.ts`, `src/entrypoints/sdk/coreSchemas.ts` | `clawin-core` + `clawin-bootstrap` + `clawin-tools` | `Parity Pending` | `--print`、prompt source 解析、`text/json/stream-json` 输出、stdin `stream-json` 协议、permission control request/response/cancel、busy、interrupt、`--continue` / `--resume` in print mode、headless golden fixture | `DIFF-2026-001` | 当前仅覆盖本地 stdio JSON 与 headless 非交互链路；多客户端桥接、网络 transport 与 remote session 管理仍待 `Phase 7B2` |
| Remote Control / Bridge | `src/remote/*`, `src/bridge/*`, `src/cli/transports/*`, `src/commands/bridge/*` | `clawin-integrations` + `clawin-bootstrap` + `clawin-ui` | `Parity Pending` | `remote-control` / `rc`、REPL `/remote-control` / `/rc`、bridge session、transport reconnect、remote resume / pointer recovery、bridge status rendering、busy / interrupt / permission mediation、bridge golden fixture | `DIFF-2026-001` | 当前默认 connector 仍为 unavailable；真实 backend/auth、多 session bridgeMain loop 与更完整多客户端协作仍待后续阶段 |

## Phase 8 审计总览

Phase 8E 起，`M8` 的发布门禁以 [release-checklist.md](/Users/goya/Repo/claude/clawin/docs/status/release-checklist.md) 为汇总真源。  
在 release checklist 显式收口前，一级子系统即使局部证据增强，也不自动升为 `Parity Verified`。
当前 release gate 已归档一轮本地 macOS fixed smoke 全通过结果；但当前远端只跟踪 `origin/main`，且 `origin/main` 尚无 `.github/workflows/rust-ci.yml`，所以 Linux/Windows 还没有可归档的 fixed-smoke run，一级子系统状态继续保持 `Parity Pending`。
`Skills / Plugins`、`Worktree / Session / Resume`、`Structured IO / Headless` 与 `Remote Control / Bridge` 的首批正式复核已经完成，但都因缺三平台 release archive 未被升级。

| 子系统 | 当前结论 | 当前证据 | Phase 8 必补 | `M8` 后扩展项 |
| --- | --- | --- | --- | --- |
| Bootstrap / Entrypoint | V1 启动最小闭环已达成，但仍是 `Parity Pending` | `crates/clawin/tests/cli_smoke.rs`、`crates/clawin-bootstrap/tests/interactive_session.rs`、`crates/clawin-bootstrap/tests/non_interactive_session.rs` | 补 startup 行为来源样本、golden 和三平台启动证据 | 更完整 startup side effects |
| Config / Settings / Persistence | 读取、迁移与失败路径骨架可用，但仍属最小实现 | `crates/clawin-config/tests/startup_config.rs` | 补 settings 行为样本、平台路径结论与来源证据 | 更多上游字段与 merge 语义 |
| Commands | 参考命令可用，但命令面对标仍不完整 | `crates/clawin-commands/tests/registry.rs`、`mcp.rs`、`resume.rs`、`skills_plugins.rs` | 固定 V1 命令边界并补 sourcemap / golden 证据 | 更多命令与动态来源 |
| Tools | 参考工具闭环可用，但仍是样本实现 | `crates/clawin-tools/tests/file_read.rs`、`mcp.rs`、`worktree.rs`、`permission_resolver.rs` | 补当前纳入 V1 的工具结果 fixture、失败路径与来源证据 | 更多高副作用工具 |
| Conversation Engine | 最小 turn loop 已达成，但并非完整 query 对标 | `crates/clawin-engine/tests/conversation_engine.rs` 与 fixtures | 补事件序列、失败路径与 V1 边界证据 | 真实 provider 与更多 query 语义 |
| TUI / REPL / Screens | 最小 REPL 闭环已达成，但 UI 语义仍是精简版 | `crates/clawin-ui/tests/repl.rs`、`crates/clawin-platform/tests/terminal_session.rs` | 补 snapshot / 三平台终端证据与当前 UI 边界说明 | 更多 screen、history、scrollback |
| MCP | stdio MCP 最小闭环已达成 | `crates/clawin-integrations/tests/mcp_manager.rs`、`fake_stdio_process.rs`、`crates/clawin-bootstrap/tests/mcp_bootstrap.rs`、`crates/clawin-tools/tests/mcp.rs` | 补 `/mcp`、resource tools、动态 tool 的来源样本与平台结论 | 更多 transport 与通知刷新 |
| Skills / Plugins | 运行时加载和动态命令导出已可用，但仍是最小闭环 | `crates/clawin-integrations/tests/skills_plugins.rs`、`crates/clawin-commands/tests/skills_plugins.rs`、`crates/clawin-bootstrap/tests/skills_plugins_bootstrap.rs`、`crates/clawin-commands/tests/fixtures/skills_normalized_output.txt`、`skill_command_display_output.txt`、`plugin_precedence_output.txt` | 补 sourcemap 来源摘录、三平台结论与 `Parity Verified` 升级证据；display/token、precedence、plugin failure 的公共文本输出已进入 fixture 基线 | marketplace/install/update 与模板生态 |
| Worktree / Session / Resume | transcript 真源、恢复失败路径、worktree 生命周期与 restored runtime/file-read 已进入 fixture 基线，但仍未形成 `Parity Verified` 证据包 | `crates/clawin-config/tests/session_store.rs`、`crates/clawin-config/tests/fixtures/*.jsonl`、`crates/clawin-commands/tests/resume.rs`、`crates/clawin-commands/tests/fixtures/resume_*.txt`、`crates/clawin-bootstrap/tests/resume_session.rs`、`crates/clawin-tools/tests/worktree.rs`、`crates/clawin-tools/tests/fixtures/*worktree*.json`、`crates/clawin-platform/tests/git_worktree.rs` | 补 sourcemap 来源摘录、三平台路径结论与 `Parity Verified` 升级证据 | 更多恢复语义与退出交互 |
| Structured IO / Headless | `--print` 主链路可用，text/json/stream-json、permission、busy、interrupt 与 print-mode resume/continue 的 fixture 基线已形成，但当前正式发布判定仍受三平台 smoke matrix 阻塞 | `crates/clawin/tests/cli_smoke.rs`、`crates/clawin/tests/fixtures/print_help_text.txt`、`print_help_json.json`、`print_help_stream_json.jsonl`、`crates/clawin-bootstrap/src/print.rs` 内测试、`crates/clawin-bootstrap/tests/fixtures/headless_stream_text_delta.jsonl`、`headless_permission_allow.jsonl`、`headless_permission_deny.jsonl`、`headless_permission_interrupt.jsonl` | 补 sourcemap 来源摘录、三平台 smoke 结论与 `Parity Verified` 升级证据 | 更丰富 host / bridge 集成 |
| Remote Control / Bridge | bridge 最小闭环已达成，pointer anchor、status 输出、busy/cancel/reconnect 与 CLI failure path 已进入 fixture 基线，但当前正式发布判定仍受 REPL attached 证据与三平台 smoke matrix 阻塞 | `crates/clawin-integrations/tests/bridge.rs`、`crates/clawin-integrations/tests/fixtures/bridge_pointer_sample.json`、`crates/clawin-bootstrap/tests/remote_control.rs`、`crates/clawin-bootstrap/tests/fixtures/remote_control_permission_allow.jsonl`、`remote_control_permission_interrupt.jsonl`、`remote_control_busy.jsonl`、`remote_control_status_connected.txt`、`remote_control_status_failed.txt`、`crates/clawin-commands/tests/remote_control.rs`、`crates/clawin-commands/tests/fixtures/remote_control_status_output.txt`、`crates/clawin/tests/cli_smoke.rs`、`crates/clawin-ui/tests/repl.rs` | 补 sourcemap 来源摘录、REPL attached 证据与三平台 smoke 结论 | 真实 backend/auth、多会话 bridge |

## 子系统最小验收模板

每个二级条目补齐时，至少需要回答下列问题:

1. 对应哪一个上游公开行为
2. 由哪个 crate 负责实现
3. 有哪些黄金路径
4. 有哪些失败路径
5. 如何做对标验证
6. 是否存在差异 ID

Phase 8 审计补充规则:

7. 当前实现是“V1 已足够的最小闭环”，还是“仅有样本实现”
8. 若尚不能升为 `Parity Verified`，阻塞项是什么
9. 哪些未实现能力属于 `M8` 阻塞，哪些明确留到 `M8` 之后

## Phase 3 二级验收注记

| 条目 | 当前状态 | Rust 归属 | 验收说明 | 差异 |
| --- | --- | --- | --- | --- |
| command registry / alias / lazy load | `Parity Pending` | `clawin-commands` | `/help` 与 `/?` 解析到同一 canonical command，执行时才加载 handler，unknown command 稳定失败 | `DIFF-2026-001` |
| `/help` reference command | `Parity Pending` | `clawin-commands` | 输出稳定文本 fixture，用于锁定 command 执行结果协议 | `DIFF-2026-001` |
| tool schema / validation / permission decision | `Parity Pending` | `clawin-tools` | `file_path` 缺失稳定报错，project root 外路径触发 `ask` 并在非交互模式稳定拒绝 | `DIFF-2026-001` |
| `file_read` reference tool | `Parity Pending` | `clawin-tools` | 仅支持 UTF-8 文本读取，支持 `offset/limit`，PDF/图片/二进制稳定返回不支持错误 | `DIFF-2026-001` |
| transcript state / cross-submit persistence | `Parity Pending` | `clawin-engine` | 同一 `ConversationEngine` 上连续 `submit_message` 会保留 user/assistant/tool transcript，并在 deterministic compact 后保留结构化摘要段 | `DIFF-2026-001` |
| `submit_message` turn loop / streaming event protocol | `Parity Pending` | `clawin-engine` | prompt 通过 typed `EngineEvent` 增量输出 text delta、assistant completion、turn/session finish；`/help` 走 fast-path 且不触发 model driver | `DIFF-2026-001` |
| `file_read` tool continuation | `Parity Pending` | `clawin-engine` + `clawin-tools` | model -> tool -> model 两段闭环可跑通，tool permission 与 tool result 会进入 transcript 并产出稳定 event 序列 fixture | `DIFF-2026-001` |
| token budget continuation / stop | `Parity Pending` | `clawin-engine` | token budget 下可稳定触发 continuation 建议与 `BudgetStopped` 停止路径，diminishing returns 规则已有接口位置 | `DIFF-2026-001` |
| deterministic compaction hook | `Parity Pending` | `clawin-engine` | transcript 超阈值时会产出 `CompactSummary` 并发出 `CompactionApplied` 事件；当前不做真实语义 compact | `DIFF-2026-001` |
| cancel / failure path | `Parity Pending` | `clawin-engine` | 取消请求可在 model 调用前稳定落成 `Cancelled`；model/tool failure 会发出 `EngineFailed` 并结构化上浮 | `DIFF-2026-001` |

## Phase 5 二级验收注记

| 条目 | 当前状态 | Rust 归属 | 验收说明 | 差异 |
| --- | --- | --- | --- | --- |
| interactive no-arg routing | `Parity Pending` | `clawin-bootstrap` + `clawin-ui` | interactive terminal 下无参数执行进入真实 REPL；non-interactive no-arg 保持稳定 placeholder 且不初始化 TUI | `DIFF-2026-001` |
| terminal session abstraction | `Parity Pending` | `clawin-platform` | raw mode / alternate screen 生命周期、键盘事件、resize、test double 全部收敛到 `TerminalSession`，`clawin-ui` 不直接写平台分支 | `DIFF-2026-001` |
| REPL event rendering | `Parity Pending` | `clawin-ui` | REPL 直接消费 `EngineEvent` 构建 transcript/status view model，slash command、tool progress、cancel/error 可稳定显示 | `DIFF-2026-001` |
| slash command in REPL | `Parity Pending` | `clawin-ui` + `clawin-engine` + `clawin-commands` | `/help` 在真实 REPL 内可执行，走 command fast-path，且不触发 model driver | `DIFF-2026-001` |
| unavailable-driver prompt path | `Parity Pending` | `clawin-bootstrap` + `clawin-ui` | 默认 interactive path 注入 `UnavailableModelDriver`，prompt 提交稳定返回受控失败/占位反馈，不冒充真实 provider 能力 | `DIFF-2026-001` |
| cancel / resize path | `Parity Pending` | `clawin-ui` + `clawin-platform` + `clawin-engine` | 运行态 `Ctrl-C` 触发 cancel 并回到输入态；空闲态 `Ctrl-C` 退出 REPL；resize 会刷新 view model 与渲染尺寸 | `DIFF-2026-001` |
| TUI snapshot acceptance | `Parity Pending` | `clawin-ui` | 使用 `ratatui` test backend 覆盖空会话、`/help`、streaming、tool 执行中、error/cancel 等稳定 snapshot | `DIFF-2026-001` |

## Phase 6A 二级验收注记

| 条目 | 当前状态 | Rust 归属 | 验收说明 | 差异 |
| --- | --- | --- | --- | --- |
| `mcpServers` merge / config validation | `Parity Pending` | `clawin-integrations` + `clawin-config` | 从 `~/.clawin/settings.json` 与 `<project_root>/.clawin/settings.json` 顶层 `mcpServers` 读取；项目级按 server name 覆盖全局；顶层非 object 时 bootstrap 前稳定失败；单 server 非法或非 `stdio` transport 进入 `Failed` 状态但不阻断启动 | `DIFF-2026-001` |
| stdio connect / initialize / snapshot | `Parity Pending` | `clawin-integrations` + `clawin-platform` | 启动时 eager connect，每个 server 固定 `5s` 初始化超时；成功后缓存 `server info/capabilities`、`tools/list`、`resources/list` 结果；失败保留 `last_error` 且不阻断 REPL 启动；workspace 内 fake stdio MCP server 已进入测试基线 | `DIFF-2026-001` |
| `/mcp list` / `/mcp reload` | `Parity Pending` | `clawin-commands` + `clawin-bootstrap` + `clawin-integrations` | `/mcp` 等价 `/mcp list`；`/mcp list` 输出 scope、transport、status、tool_count、resource_count、last_error；`/mcp reload` 可重新连接并刷新 tools/resources 快照 | `DIFF-2026-001` |
| MCP dynamic tool naming | `Parity Pending` | `clawin-integrations` + `clawin-tools` | 远端 MCP tool 名称固定为 `mcp__{normalized_server_name}__{normalized_tool_name}`，normalization 规则为非 `[a-zA-Z0-9_-]` 统一替换为 `_` | `DIFF-2026-001` |
| `list_mcp_resources` | `Parity Pending` | `clawin-tools` + `clawin-integrations` | 输入固定为 `{ server?: string }`；可读取已连接 server 的 resource 快照；missing/disconnected server 返回稳定结构化错误 | `DIFF-2026-001` |
| `read_mcp_resource` | `Parity Pending` | `clawin-tools` + `clawin-integrations` | 输入固定为 `{ server: string, uri: string }`；当前仅支持 text content；二进制内容稳定返回 `unsupported_binary_resource`；不做落盘持久化 | `DIFF-2026-001` |
| bootstrap + engine + REPL orchestration | `Parity Pending` | `clawin-bootstrap` + `clawin-engine` + `clawin-ui` + `clawin-tools` | bootstrap 在 config 后装配 `McpManager`；`RuntimeCapabilities.mcp_available` 由 merged `mcpServers` 是否为空决定；engine 通过现有 `ToolRegistry` 调用 MCP tools；REPL 只需显示 `/mcp` 文本结果与 MCP tool 事件 | `DIFF-2026-001` |

## Phase 6B 二级验收注记

| 条目 | 当前状态 | Rust 归属 | 验收说明 | 差异 |
| --- | --- | --- | --- | --- |
| skills directory discovery / frontmatter parsing | `Parity Pending` | `clawin-integrations` | 从 `~/.clawin/skills/` 与 `<project_root>/.clawin/skills/` 递归发现 `SKILL.md`；frontmatter 至少解析 `name`、`description`、`tools`；非法 frontmatter 或空 markdown 进入解析错误列表但不阻断其他 skill 加载 | `DIFF-2026-001` |
| skill override precedence / snapshot | `Parity Pending` | `clawin-integrations` | 发现顺序固定为 `project > user`；同名 skill 由项目级整份覆盖全局级；输出 `LoadedSkillsSnapshot`，保留 skill 元数据、原始 markdown、来源与错误列表 | `DIFF-2026-001` |
| dynamic skill command export / `/skills` | `Parity Pending` | `clawin-commands` + `clawin-integrations` | skills 会导出动态 slash commands；`/skills` 输出显示名、来源与描述摘要，并在显示名与 token 不同的时候显式展示 normalized slash-command token；执行 `/{normalized_token}` 或 `/{plugin_id}:{normalized_token}` 时返回稳定文本/prompt scaffold，不接真实模型增强逻辑；当前公开输出由 `skills_normalized_output.txt` 与 `skill_command_display_output.txt` 锁定 | `DIFF-2026-001` |
| plugin manifest loading / status view | `Parity Pending` | `clawin-integrations` + `clawin-commands` | 从 `~/.clawin/plugins/` 与 `<project_root>/.clawin/plugins/` 递归发现 `plugin.json`；消费最小 manifest 字段并生成 runtime snapshot；`/plugin` 输出 plugin 来源、状态、贡献摘要与失败信息 | `DIFF-2026-001` |
| plugin-contributed command loading | `Parity Pending` | `clawin-integrations` + `clawin-commands` | plugin commands、plugin skills 与 builtin commands 共用同一 registry；来源标记可区分 builtin / dynamic / plugin；重复 command 名冲突会稳定进入失败/忽略路径 | `DIFF-2026-001` |
| plugin-contributed MCP merge | `Parity Pending` | `clawin-integrations` + `clawin-bootstrap` | plugin 声明的 MCP servers 以 `plugin:{plugin_id}:{server_name}` 命名空间并入现有 `McpManager`；不会引入新的独立 transport 层；`/mcp list` 可看到 plugin 贡献项 | `DIFF-2026-001` |
| non-blocking plugin failure path | `Parity Pending` | `clawin-integrations` + `clawin-bootstrap` + `clawin-ui` | 单个 plugin manifest 非法、内容缺失或解析失败时，不阻断整体启动；该 plugin 进入 `Failed` 或 `Ignored` 状态并带稳定错误信息，REPL 仍可继续启动和列出结果；`/plugin` 的 ignored/failed 公共输出由 `plugin_precedence_output.txt` 锁定 | `DIFF-2026-001` |
| bootstrap + engine + REPL orchestration | `Parity Pending` | `clawin-bootstrap` + `clawin-engine` + `clawin-ui` + `clawin-commands` | bootstrap 装配顺序固定为 config -> MCP -> skills -> plugins -> registry；engine 继续复用 slash command fast-path；REPL 只需显示动态 skill/plugin commands 已可用及稳定执行结果 | `DIFF-2026-001` |

## Phase 7A 二级验收注记

| 条目 | 当前状态 | Rust 归属 | 验收说明 | 差异 |
| --- | --- | --- | --- | --- |
| session path layout | `Parity Pending` | `clawin-config` + `clawin-platform` | session transcript 固定落在 `~/.clawin/projects/<sanitized-active-project-root>/<session_id>.jsonl`；目录 key 由 active project path 做文件系统安全归一化，不复用 config `project_key` | `DIFF-2026-001` |
| JSONL transcript schema `1` | `Parity Pending` | `clawin-config` + `clawin-core` | JSONL 是唯一 session 真源；最小 entry 固定为 `session_header`、`message`、`last_prompt`、`worktree_state`；unknown entry 忽略保留，known entry 非法或 schema version 不支持时稳定失败 | `DIFF-2026-001` |
| `/resume` / `/continue` command flow | `Parity Pending` | `clawin-commands` + `clawin-core` + `clawin-ui` | `/resume` 无参数列出当前 active project + same-repo worktrees 范围内的 recent sessions；`/continue` 作为 alias；命中单个 session 时通过 `CommandEffect::ResumeSession` 触发 REPL session hot-swap，而不是在 command crate 内直接篡改 engine | `DIFF-2026-001` |
| CLI `--continue` / `--resume` | `Parity Pending` | `clawin-bootstrap` | `--help` / `--version` / bad-flag 继续走 fast-path；`--continue` 恢复当前 scope 最新 session；`--resume <session-id-or-jsonl-path>` 支持按 id、搜索词和显式 transcript path 进入 restore 路径 | `DIFF-2026-001` |
| same-repo worktree session search | `Parity Pending` | `clawin-config` + `clawin-platform` | recent/resume 默认只搜索当前 active project 与 same-repo worktrees；跨项目恢复只允许显式 `.jsonl` path，不做全局 picker | `DIFF-2026-001` |
| `EnterWorktree` / `ExitWorktree` | `Parity Pending` | `clawin-tools` + `clawin-bootstrap` + `clawin-platform` | `EnterWorktree` 仅在 git 仓库内创建 `.clawin/worktrees/<slug>` 下的 session-owned worktree，并更新 runtime + `worktree_state`；`ExitWorktree` 支持 `keep|remove`，dirty worktree 在未显式 `discard_changes` 时稳定拒绝；成功/失败结果由 worktree JSON fixtures 锁定 | `DIFF-2026-001` |
| resume / restore failure path | `Parity Pending` | `clawin-bootstrap` + `clawin-config` + `clawin-commands` | 未命中、多命中、非法 JSONL、unsupported schema、非法 known entry、缺失 transcript path 都返回稳定错误，不进入半恢复状态；`/resume` 文本输出由 `resume_*.txt` fixtures 锁定 | `DIFF-2026-001` |
| restored runtime orchestration | `Parity Pending` | `clawin-bootstrap` + `clawin-engine` + `clawin-ui` | restored path 需重建 runtime snapshot、engine transcript、active worktree state 与 interrupted notice；恢复后同一个 REPL 会话内仍可继续 submit prompt 或 tool call；当前已覆盖 transcript anchor、active worktree runtime 与相对 `file_read` 解析 | `DIFF-2026-001` |

## Phase 7B1 二级验收注记

| 条目 | 当前状态 | Rust 归属 | 验收说明 | 差异 |
| --- | --- | --- | --- | --- |
| `--print` CLI surface / prompt source parsing | `Parity Pending` | `clawin-bootstrap` | 新增 `--print`、`--input-format`、`--output-format`、`--verbose` 与 print-mode positional prompt；`text` 模式下 positional prompt 与 piped stdin 二选一；`stream-json` 模式禁止 positional prompt；`--output-format=stream-json` 必须同时带 `--verbose` | `DIFF-2026-001` |
| `text/json/stream-json` output modes | `Parity Pending` | `clawin-bootstrap` + `clawin-core` | `text` 输出稳定人类可读文本；`json` 输出单个 `result`/`error` JSON；`stream-json` 输出 `session_started`、`stream_event`、`control_request`、`control_cancel_request`、`result`、`error`、`keep_alive` 的 line-delimited JSON | `DIFF-2026-001` |
| stdin `stream-json` protocol | `Parity Pending` | `clawin-core` + `clawin-bootstrap` | stdin 固定接受 `user`、`control_request(interrupt)`、`control_response`、`keep_alive`；同一 headless session 支持连续多次 `user` submit 并保留 transcript；运行中收到新的 `user` 输入时稳定返回 `busy` 错误 | `DIFF-2026-001` |
| permission control request / response | `Parity Pending` | `clawin-core` + `clawin-bootstrap` + `clawin-tools` | `PermissionBehavior::Ask` 在 `--print --input-format=stream-json` 下通过 stdout `control_request` 发起，host 通过 stdin `control_response` 回应；每次请求都有唯一 `request_id`；未匹配、格式非法、EOF、重复响应与失效响应都稳定拒绝或上浮结构化错误 | `DIFF-2026-001` |
| interrupt / control cancel path | `Parity Pending` | `clawin-bootstrap` + `clawin-tools` + `clawin-engine` | 运行中通过 stdin `control_request(interrupt)` 触发取消；pending permission 在 turn 结束或取消时会发出 `control_cancel_request`；plain text print 模式不承担 host permission 交互 | `DIFF-2026-001` |
| print-mode resume / continue | `Parity Pending` | `clawin-bootstrap` + `clawin-config` + `clawin-engine` | `--print` 与 `--continue` / `--resume <id|jsonl-path>` 兼容；恢复后继续复用现有 session transcript persistence、`last_prompt` 落盘与 worktree/runtime 恢复链路 | `DIFF-2026-001` |
| headless golden fixtures / fake-driver acceptance | `Parity Pending` | `clawin-bootstrap` + `clawin-engine` + `clawin-tools` | 覆盖 text-only `stream-json` 事件序列、permission request -> host response -> tool complete、permission cancel、`json` 最终结果对象、text 输出样本与 print-mode `--continue` / `--resume` smoke；当前公共样本固定为 `print_help_text.txt`、`print_help_json.json`、`print_help_stream_json.jsonl` 与 `headless_permission_*.jsonl` | `DIFF-2026-001` |

## Phase 7B2 二级验收注记

| 条目 | 当前状态 | Rust 归属 | 验收说明 | 差异 |
| --- | --- | --- | --- | --- |
| standalone `remote-control` / `rc` CLI surface | `Parity Pending` | `clawin-bootstrap` | 顶层子命令 `remote-control`，alias `rc`；支持 `[name]` 与 `--continue` 且二者互斥；`--help`、`--version`、bad flag、`--print` 规则保持稳定；standalone worker 以前台单 session 运行，不引入多 session spawn loop；当前 CLI help 与 unavailable/no-pointer 失败路径由 `crates/clawin/tests/cli_smoke.rs` 锁定 | `DIFF-2026-001` |
| bridge pointer persistence / `--continue` recovery | `Parity Pending` | `clawin-integrations` + `clawin-bootstrap` + `clawin-platform` | pointer 固定落在 `~/.clawin/projects/<sanitized-active-project-root>/bridge-pointer.json`；TTL `4h`；`remote-control --continue` 在当前 active project + same-repo worktrees 范围内读取 freshest valid pointer，并在 stale/invalid 时稳定清理失败；pointer transcript path 优先复用 session transcript anchor，公共样本由 `bridge_pointer_sample.json` 锁定 | `DIFF-2026-001` |
| standalone bridge host / structured IO reuse | `Parity Pending` | `clawin-bootstrap` + `clawin-core` | standalone bridge 复用现有 structured/headless host 与 `StructuredInputMessage` / `StructuredOutputMessage` 协议；slash command、tool continuation、permission request/response、interrupt 与 transcript persistence 都沿用 `Phase 7B1` 主链路 | `DIFF-2026-001` |
| REPL `/remote-control` / `/rc` current-session bridge | `Parity Pending` | `clawin-ui` + `clawin-core` | REPL 内 `/remote-control [name]`、`/remote-control status`、`/remote-control stop` 复用现有 slash command fast-path；bridge worker 绑定当前 live REPL session，而不是创建第二个 engine；远端 `/help` 可打进当前会话并回传稳定结果；当前 `/remote-control status` 文本输出由 `remote_control_status_output.txt` 锁定 | `DIFF-2026-001` |
| bridge transport reconnect / terminal state | `Parity Pending` | `clawin-integrations` | `BridgeManager` 负责 `ready -> connected -> reconnecting -> failed -> stopped` 状态机；断连后采用 `2s` 初始、`30s` 上限、`10m` give-up 的重连策略；give-up 后进入 `Failed` 但不杀掉本地 REPL/session；当前已覆盖 repeated start 复用现有 worker 与 `connected -> reconnecting -> failed` 的状态转换 | `DIFF-2026-001` |
| remote/local busy / interrupt / permission mediation | `Parity Pending` | `clawin-ui` + `clawin-bootstrap` + `clawin-tools` + `clawin-engine` | standalone 与 REPL attached bridge 均固定单 active turn；运行中新的 remote `user` 输入稳定返回 `busy`；remote `interrupt` 可取消当前 turn；remote-originated `PermissionBehavior::Ask` 通过 bridge `control_request` / `control_response` / `control_cancel_request` 闭环处理；当前公共事件样本由 `remote_control_permission_allow.jsonl`、`remote_control_permission_interrupt.jsonl` 与 `remote_control_busy.jsonl` 锁定 | `DIFF-2026-001` |
| fake connector / fake backend acceptance | `Parity Pending` | `clawin-integrations` + `clawin-bootstrap` + `clawin-ui` | fake connector/backend 覆盖 standalone `/help`、REPL attached `/help`、pointer recovery、transport drop/reconnect、permission ask、tool continuation 与 unavailable prompt path；standalone status 文本由 `remote_control_status_connected.txt` 与 `remote_control_status_failed.txt` 锁定；三平台持续保持 build/test/smoke | `DIFF-2026-001` |

## 差异记录约束

- 差异 ID 格式: `DIFF-YYYY-NNN`
- 本表出现 `Accepted Difference` 时，必须链接到 ADR 或完整差异描述
- 没有差异 ID 的偏离，一律视为未批准
