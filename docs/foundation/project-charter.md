# Clawin 项目宪章

- 状态: Accepted
- 生效日期: 2026-04-08
- 适用范围: 全仓库

## 1. 项目目标

Clawin 是一个基于 Rust 重构的终端代理式编码产品。V1 的明确目标是对标 Claude Code `v2.1.88` 公开包的外部行为，并在此基础上建立可持续扩展的 Rust 工程架构。

这里的“对标”指:

- CLI 行为与交互语义
- 命令与工具体系的公开能力
- 会话、上下文、resume、worktree、MCP 等公开流程
- 配置、权限、状态持久化、非交互模式等用户可感知行为

这里的“重构”不指逐文件翻译 TypeScript，而是保留上游子系统边界、职责和行为语义，在 Rust 内部按更稳健的方式重写实现。

## 2. V1 范围

### 2.1 纳入范围

- 对标 Claude Code `2.1.88` 公开包行为
- 三平台首发: macOS、Linux、Windows
- 单主二进制交付形态
- Rust workspace 和分层 crate 架构
- Clawin 命名空间下的配置目录、项目约定文件和状态持久化
- 对标测试、golden fixture、三平台回归矩阵

### 2.2 不纳入范围

- `ant-only`、内部命令、仅内部 feature flag 可达的实现分支
- 任何未在公开包行为中可验证的内部 Anthropic 系统
- “只为更像上游目录结构”而做的逐文件映射
- 与 V1 公开行为无关的产品差异化功能

## 3. 真源优先级

Clawin 的所有实现、评审和验收必须遵循以下真源优先级:

1. Claude Code `2.1.88` 公开包的可观察行为
2. `docs/claude-code-sourcemap-main/restored-src/src/` 中的还原 TypeScript 源码
3. `docs/claude-code-sourcemap-main/package/` 中的包内元数据与 README
4. 仓库内已接受的 ADR、对标矩阵和标准文档
5. 明确标注为假设的工程推断

若多个真源冲突，必须在 ADR 或差异记录中明确说明取舍依据。

## 4. 产品身份与命名

- CLI 命令名固定为 `clawin`
- 主配置目录固定为 `~/.clawin`
- 项目级目录固定为 `.clawin/`
- 项目级主说明文件固定为 `CLAWIN.md`
- V1 不兼容读取 `.claude`、`CLAUDE.md` 或 `~/.claude`

保留根层 [CLAUDE.md](/Users/goya/Repo/claude/clawin/CLAUDE.md) 仅用于兼容部分工具链或协作环境，不作为正式工程规范来源。

## 5. 架构原则

- 子系统可追溯: 每个 Rust 子系统必须能映射回 Claude Code 的上游职责域
- 行为优先: 公开行为的正确性优先于内部实现的“Rust 审美”
- 平台隔离: 所有 OS 差异必须经由平台抽象层，不得向业务层泄漏
- 状态显式化: 禁止继续堆积隐式全局状态，跨 turn 和跨进程状态必须显式建模
- 文档先行: 新子系统实现前必须先冻结对标条目、验收标准和必要 ADR

## 6. 验收标准

一个子系统只有在满足以下条件后，才可视为进入 `Parity Verified`:

- 对标矩阵存在明确条目
- 上游入口与 Rust 归属关系已记录
- 行为验收测试已落地
- 三平台相关差异已被验证或被明确登记
- 所有故意偏差都有差异 ID，并已链接到 ADR 或差异区
- 状态文档与风险文档已同步更新

## 7. 统一状态

### 7.1 对标状态

- `Not Started`: 尚未形成足够的实施基线
- `Spec Ready`: 需求、边界、验收已可支持开工
- `In Progress`: 已开始实现或迁移
- `Parity Pending`: 代码已基本完成，等待对标与平台验证
- `Parity Verified`: 行为与验收通过
- `Accepted Difference`: 有意识地与上游不同，且已被批准

### 7.2 ADR 状态

- `Proposed`
- `Accepted`
- `Superseded`
- `Rejected`

## 8. 差异管理

所有故意偏离上游公开行为的设计都必须拥有唯一差异 ID，格式如下:

- `DIFF-YYYY-NNN`

每个差异至少记录:

- 差异 ID
- 关联子系统
- 上游行为
- Clawin 行为
- 偏离原因
- 风险
- 验收与回归要求

差异必须出现在以下二者之一:

- 对应 ADR
- 对标矩阵中的差异栏，并链接到更完整的 ADR

## 9. 变更控制

- 新子系统开工前，必须先建立对标条目与验收标准
- 影响 workspace、运行时、平台抽象、状态模型、持久化、测试策略、feature 策略的变更，必须先写 ADR
- 每个迁移 PR 必须同时更新代码、对标矩阵、测试、状态文档
- 不允许“先实现，后补标准”

## 10. 文档权威性

仓库内 Markdown 文档是唯一正式基线。任何口头结论、会话结论或临时聊天结论，只有在落入以下文档后才算生效:

- 宪章
- 对标矩阵
- ADR
- 工程标准
- 路线图
- 状态文档

## 11. 当前参考入口

初始对标分析应优先从以下上游入口开始:

- `src/main.tsx`
- `src/setup.ts`
- `src/QueryEngine.ts`
- `src/query.ts`
- `src/tools.ts`
- `src/commands.ts`
- `src/utils/config.ts`
- `src/bootstrap/state.ts`
- `src/services/mcp/*`

这些入口不是完整实现清单，但构成 V1 首轮分层与路线图的主骨架。
