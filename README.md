# Clawin

Clawin 是一个基于 Rust 重构的终端代理式编码产品，V1 目标是对标 Claude Code `v2.1.88` 的公开包行为，同时以 Rust 的工程化方式重建其核心架构、运行时和平台适配层。

当前仓库已经完成治理基线、workspace 骨架、`bootstrap/config` 首轮闭环，以及 `commands/tools/engine` 的 `M3` 最小闭环。所有工程决策、对标范围、差异处理和验收规则都以仓库内文档为准，不依赖口头说明。

## 当前阶段

- 阶段: 已进入 `M3`，`commands/tools/engine` 最小闭环已完成，下一步推进 engine turn loop 与更完整的行为对标
- 上游参考语料: [docs/claude-code-sourcemap-main/README.md](/Users/goya/Repo/claude/clawin/docs/claude-code-sourcemap-main/README.md)
- 对标目标: Claude Code `2.1.88` 公开包行为
- 产品身份: `clawin`
- 配置命名: `~/.clawin`、`.clawin/`、`CLAWIN.md`
- 当前差异基线: `DIFF-2026-001`

## 阅读顺序

1. [docs/foundation/project-charter.md](/Users/goya/Repo/claude/clawin/docs/foundation/project-charter.md)
2. [docs/parity/parity-matrix.md](/Users/goya/Repo/claude/clawin/docs/parity/parity-matrix.md)
3. [docs/adr/README.md](/Users/goya/Repo/claude/clawin/docs/adr/README.md)
4. [docs/standards/rust-engineering.md](/Users/goya/Repo/claude/clawin/docs/standards/rust-engineering.md)
5. [docs/standards/testing-and-parity.md](/Users/goya/Repo/claude/clawin/docs/standards/testing-and-parity.md)
6. [docs/plans/master-roadmap.md](/Users/goya/Repo/claude/clawin/docs/plans/master-roadmap.md)
7. [docs/status/parity-status.md](/Users/goya/Repo/claude/clawin/docs/status/parity-status.md)

## 文档地图

- `docs/foundation/`: 项目宪章与总原则
- `docs/parity/`: Claude Code 对标矩阵与差异基线
- `docs/adr/`: Architecture Decision Records
- `docs/standards/`: 工程规范、测试规范、变更控制
- `docs/plans/`: 执行路线图与实施计划
- `docs/status/`: 对标进度、风险登记和阻塞信息
- `docs/claude-code-sourcemap-main/`: 上游 TypeScript 还原语料，仅作为研究与对标参考

## 工作原则

- 公开行为对齐优先于内部实现自由度
- 任何故意偏离上游公开行为的变更，必须先有 ADR 或差异记录
- 新子系统开工前，必须先在对标矩阵中拥有条目和验收规则
- 每个迁移 PR 必须同时更新代码、文档、测试和状态文档
