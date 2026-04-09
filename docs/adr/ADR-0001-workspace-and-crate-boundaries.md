# ADR-0001: Workspace 与 crate 边界

- 状态: Accepted
- 日期: 2026-04-08

## 背景

Clawin 需要对标 Claude Code `2.1.88` 的公开包行为，同时避免把 TypeScript/Bun 的历史实现细节直接照搬到 Rust。如果缺少明确的 workspace 和 crate 边界，后续实现很容易退化成“大杂烩二进制”。

## 决策

采用 Cargo workspace + 单主二进制架构。

首批 crate 边界固定为:

- `clawin-core`
  - 公共类型、ID、错误、事件、共享 trait、协议数据模型
- `clawin-bootstrap`
  - 启动流程、CLI 参数、会话创建、模式路由、进程级 runtime 装配
- `clawin-config`
  - 全局配置、项目配置、持久化、迁移、文档目录发现规则
- `clawin-engine`
  - 会话主循环、streaming、turn state、compact、continuation、budget
- `clawin-tools`
  - tool trait、注册表、权限、执行编排、工具适配器
- `clawin-commands`
  - slash command 注册表、prompt/local command 语义、命令路由
- `clawin-ui`
  - TUI、REPL、屏幕切换、键盘绑定、渲染组件
- `clawin-integrations`
  - API、MCP、skills、plugins、remote、外部协议接入
- `clawin-platform`
  - shell、TTY、secure storage、浏览器、音频、路径与 OS 差异抽象
- `clawin`
  - 主二进制，仅负责组装和调用各 crate

## 依赖规则

- 只有 `clawin` 能同时依赖多个上层 crate 来做装配
- `clawin-core` 不依赖任何业务 crate
- `clawin-platform` 只能向上提供 trait/adapter，不得反向依赖业务 crate
- `clawin-ui` 不得直接读写持久化存储，只能通过 service trait 访问
- `clawin-engine` 不得直接依赖终端渲染实现
- `clawin-commands` 和 `clawin-tools` 可以共享 `clawin-core` 的协议模型，但不允许彼此形成循环依赖

## 后果

- 优点: 子系统边界清晰，利于并行开发与对标审计
- 成本: 前期需要多写一些 trait 和装配代码
- 要求: 新增 crate 必须通过新 ADR 或扩展本 ADR 后才能引入
