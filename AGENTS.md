# Clawin Agent Working Guide

本文件面向在本仓库内工作的代理与工程执行者，定义最小执行顺序与文档依赖。它是协作入口，不替代正式规范。

## 先读什么

在开始任何实现前，按以下顺序阅读:

1. [docs/foundation/project-charter.md](/Users/goya/Repo/claude/clawin/docs/foundation/project-charter.md)
2. [docs/parity/parity-matrix.md](/Users/goya/Repo/claude/clawin/docs/parity/parity-matrix.md)
3. 与当前任务相关的 ADR
4. [docs/standards/rust-engineering.md](/Users/goya/Repo/claude/clawin/docs/standards/rust-engineering.md)
5. [docs/standards/testing-and-parity.md](/Users/goya/Repo/claude/clawin/docs/standards/testing-and-parity.md)
6. [docs/standards/change-control.md](/Users/goya/Repo/claude/clawin/docs/standards/change-control.md)
7. [docs/plans/master-roadmap.md](/Users/goya/Repo/claude/clawin/docs/plans/master-roadmap.md)

## 执行规则

- 上游参考语料固定在 [docs/claude-code-sourcemap-main/README.md](/Users/goya/Repo/claude/clawin/docs/claude-code-sourcemap-main/README.md) 及其还原源码目录
- 真源优先级以项目宪章为准
- 新子系统开工前，必须先在对标矩阵中拥有条目与验收标准
- 任何故意偏离 Claude Code `2.1.88` 公开行为的决策，必须先落 ADR 或差异登记
- 每个迁移 PR 必须同时更新代码、对标矩阵、测试和状态文档

## 文档优先级

若文档冲突，优先级如下:

1. 项目宪章
2. 已接受 ADR
3. 工程标准
4. 对标矩阵
5. 路线图与状态文档
6. 本文件

## 当前仓库约束

- 产品身份是 `clawin`
- 配置命名空间是 `~/.clawin`、`.clawin/`、`CLAWIN.md`
- 根层 [CLAUDE.md](/Users/goya/Repo/claude/clawin/CLAUDE.md) 仅作兼容占位，不承载正式规范
