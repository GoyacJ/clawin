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
| Bootstrap / Entrypoint | `src/main.tsx`, `src/setup.ts` | `clawin-bootstrap` | `Parity Pending` | `clawin --help/--version` fast-path、无参数进入 bootstrap、`SessionRuntime` 首轮装配、invalid-config 失败路径、三平台 startup smoke | 暂无 | 启动链路后续仍会接入 commands/tools，需防止 side effects 过早渗透 |
| Config / Settings / Persistence | `src/utils/config.ts`, `src/utils/settings/*`, `src/bootstrap/state.ts` | `clawin-config` + `clawin-bootstrap` | `Parity Pending` | `project_key` 归一化、`~/.clawin/config.json` 初始化、`~/.clawin/settings.json`/`.clawin/settings.json` 发现、schema `1`、migration 备份、invalid settings/config 失败路径 | `DIFF-2026-001` | settings 合并语义和更多上游字段仍待继续细化 |
| Commands | `src/commands.ts`, `src/commands/*` | `clawin-commands` | `Spec Ready` | slash command 路由、参数解析、命令输出 golden fixture | 暂无 | 命令分 prompt 型与本地型，接口设计容易漂移 |
| Tools | `src/tools.ts`, `src/Tool.ts`, `src/tools/*`, `src/services/tools/*` | `clawin-tools` | `Spec Ready` | tool schema、权限、执行编排、tool result 配对、错误路径 | 暂无 | 工具是行为对标核心，高耦合 permissions 和 engine |
| Conversation Engine | `src/QueryEngine.ts`, `src/query.ts`, `src/query/*` | `clawin-engine` | `Spec Ready` | turn loop、streaming、compact、continuation、budget、interrupt | 暂无 | 会话状态、流式事件和中断处理复杂度最高 |
| TUI / REPL / Screens | `src/ink/*`, `src/components/*`, `src/screens/*`, `src/keybindings/*` | `clawin-ui` | `Spec Ready` | REPL 交互、键盘事件、屏幕切换、文本渲染 snapshot | 允许 Rust TUI 技术栈重写实现，行为不变 | 三平台终端能力和渲染细节差异大 |
| MCP | `src/services/mcp/*`, `src/tools/MCPTool/*`, `src/tools/ListMcpResourcesTool/*`, `src/tools/ReadMcpResourceTool/*` | `clawin-integrations` + `clawin-tools` | `Spec Ready` | stdio/http/sse 连接、tool/resource 暴露、认证/失效恢复 | 暂无 | 协议、认证、超时和错误恢复都需强测试 |
| Skills / Plugins | `src/skills/*`, `src/plugins/*`, `src/utils/markdownConfigLoader.ts` | `clawin-integrations` | `Spec Ready` | 目录发现、加载顺序、约束注入、内置资源测试 | V1 仅覆盖公开包可达能力 | 需要同时处理命名空间迁移和加载规则 |
| Worktree / Session / Resume | `src/utils/worktree.ts`, `src/utils/sessionStorage.ts`, `src/utils/conversationRecovery.ts`, `src/setup.ts` | `clawin-bootstrap` + `clawin-config` + `clawin-engine` | `Spec Ready` | worktree 生命周期、resume、session 恢复、恢复失败路径 | 暂无 | 文件系统、git、tmux、路径和平台差异耦合严重 |
| Remote / Structured IO | `src/remote/*`, `src/cli/structuredIO.ts`, `src/cli/transports/*` | `clawin-integrations` + `clawin-bootstrap` | `Not Started` | 非交互传输协议、结构化消息、远程会话 golden fixture | 暂无 | V1 需求边界需在实施前进一步细化 |

## 子系统最小验收模板

每个二级条目补齐时，至少需要回答下列问题:

1. 对应哪一个上游公开行为
2. 由哪个 crate 负责实现
3. 有哪些黄金路径
4. 有哪些失败路径
5. 如何做对标验证
6. 是否存在差异 ID

## 差异记录约束

- 差异 ID 格式: `DIFF-YYYY-NNN`
- 本表出现 `Accepted Difference` 时，必须链接到 ADR 或完整差异描述
- 没有差异 ID 的偏离，一律视为未批准
