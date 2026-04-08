# ADR-0002: Runtime 与状态模型

- 状态: Accepted
- 日期: 2026-04-08

## 背景

Claude Code `2.1.88` 中存在较多 process-global 状态与懒加载单例。Rust 重构若继续依赖隐式全局状态，会使测试、并发和跨平台行为难以控制。

## 决策

Clawin 采用“双层状态模型”:

- `SessionRuntime`
  - 进程级或会话级共享依赖
  - 包含配置访问、日志、tracing、platform adapter、API/MCP 客户端工厂、clock、task spawner
- `ConversationEngine`
  - 单会话主循环实例
  - 持有消息历史、tool state、预算、compact 状态、turn 内上下文

同时固定以下规则:

- 禁止业务逻辑直接读写 `static mut` 或无约束全局单例
- 所有跨 turn 状态都必须属于 `ConversationEngine` 或其显式子结构
- 所有跨子系统共享依赖都通过 `SessionRuntime` 注入
- 后台任务必须拥有显式生命周期和取消句柄
- 事件流统一走 typed event/channel，不使用“偷偷写日志文件再读回来”的方式作为主流程同步机制

## 技术约束

- async runtime 统一为 `tokio`
- 错误必须经由结构化错误类型上浮，不允许只靠字符串比较
- tracing 上下文要能关联 session、turn、tool call、command 执行

## 后果

- 优点: 更易测试、更易做并发控制，也更接近 Rust 的安全模型
- 成本: 初期装配和接口设计更重
- 影响: 所有从上游迁移过来的“全局状态”都需要被显式重构
