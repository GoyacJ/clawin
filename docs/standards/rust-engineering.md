# Rust 工程规范

- 状态: Accepted
- 适用范围: 所有 Rust 实现

## 1. 总原则

- 行为对齐优先于实现自由
- crate 边界优先于目录整洁偏好
- 显式依赖优先于全局状态
- 可测试性优先于短期编码便利

## 2. Workspace 与 crate 规则

- crate 边界遵循 [ADR-0001](/Users/goya/Repo/claude/clawin/docs/adr/ADR-0001-workspace-and-crate-boundaries.md)
- 新 crate 必须说明:
  - 为什么不能放入现有 crate
  - 暴露哪些对外接口
  - 依赖关系为何不会造成环
- 不允许把“只是为了方便”作为跨层直接依赖的理由

## 3. 异步与并发

- async runtime 统一为 `tokio`
- 后台任务必须有:
  - 负责人
  - 生命周期
  - 取消路径
  - 错误上报路径
- 禁止在业务流程中无界 spawn
- IO-bound 和 CPU-bound 工作必须明确区分

## 4. 错误模型

- 领域错误使用结构化错误类型
- crate 边界处必须保留机器可判断的错误类别
- 用户可见错误信息与内部诊断信息要分层
- 不允许把字符串拼接当作唯一错误协议

推荐约定:

- 领域错误: `thiserror`
- 应用装配层: `anyhow` 仅用于最外层错误聚合

## 5. 日志与追踪

- 统一使用 `tracing`
- 至少要能关联以下上下文:
  - session id
  - conversation id
  - turn id
  - command name
  - tool name / tool call id
- 不允许在核心流程中只打无结构字符串日志

## 6. 平台抽象

- 平台相关逻辑只能放在 `clawin-platform`
- 其他 crate 禁止直接写平台分支来决定业务行为
- 平台 adapter 必须可替换、可 mock

## 7. 依赖引入

- 引入新第三方依赖前，必须回答:
  - 它解决的是哪一个明确问题
  - 是否会影响三平台交付
  - 是否会引入额外 native dependency
  - 是否会扩大许可证或供应链风险

## 8. API 与接口设计

- 内部接口优先用 trait + typed data，而不是字符串协议
- 事件流统一使用显式枚举和结构体
- 不允许把 JSON 当作 crate 内主交换格式，除非它本身就是外部协议

## 9. 文档要求

以下事项变更时，必须同步更新文档:

- crate 边界
- 公共状态模型
- 持久化 schema
- 测试策略
- 差异 ID

## 10. 禁止事项

- 未经批准的全局单例
- 无追踪来源的 Cargo feature
- 业务代码中直接访问用户目录常量
- 先编码再决定验收标准
