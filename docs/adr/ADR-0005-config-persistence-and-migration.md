# ADR-0005: 配置持久化与迁移策略

- 状态: Accepted
- 日期: 2026-04-08

## 背景

Claude Code `2.1.88` 的配置与状态分布在全局配置、项目配置、session 持久化和多类缓存中。Clawin 需要继承其行为语义，但使用自己的命名空间。

## 决策

Clawin 采用以下持久化策略:

- 全局配置根目录: `~/.clawin`
- 项目级目录: `.clawin/`
- 项目级主说明文件: `CLAWIN.md`
- 所有可持久化结构都带 schema version
- 配置与会话恢复数据的迁移由 `clawin-config` 集中管理

与上游对齐的关键语义:

- 保留“全局配置 + 以项目根为 key 的项目配置”双层模型
- 项目根优先使用 canonical git root；非 git 目录回退到绝对路径
- session/resume/worktree 状态允许独立持久化
- 启动时自动运行 migration，但 migration 必须可测试、可回滚、可审计

明确不做的事情:

- V1 不兼容读取 `.claude` / `CLAUDE.md` / `~/.claude`
- 不把临时运行态和长期配置混写到同一文件中

## 差异

- `DIFF-2026-001`
  - 上游行为: Claude 命名空间
  - Clawin 行为: 全量迁移为 Clawin 命名空间
  - 原因: 产品身份是显式约束

## 后果

- 优点: 命名清晰，不污染 Claude 生态目录
- 成本: 无法直接复用用户既有 Claude 本地数据
- 要求: 所有相关测试与文档都要显式体现 `DIFF-2026-001`
