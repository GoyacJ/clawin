# 变更控制规范

- 状态: Accepted
- 适用范围: 全部实现与文档变更

## 1. 必须先写 ADR 的情况

以下变更必须先有 ADR，再允许进入实现:

- workspace 或 crate 边界调整
- runtime 或状态模型调整
- TUI 技术路线变化
- 平台抽象策略变化
- 配置持久化、迁移或命名空间策略变化
- testing/parity 策略变化
- feature gating 策略变化
- 任何会影响对标口径的公开行为变化

## 2. 允许直接实施但必须更新对标矩阵的情况

- 已有架构决策下的子系统实现
- 新增测试或 fixture
- 对已有条目的细化与拆分
- 不改变口径的文档澄清

## 3. Accepted Difference 流程

如果 Clawin 需要有意识地偏离上游公开行为，必须完成以下步骤:

1. 分配差异 ID
2. 说明上游行为与 Clawin 行为
3. 记录偏离原因和风险
4. 在 ADR 或对标矩阵中登记
5. 增加回归测试锁定该差异
6. 更新状态文档

## 4. PR 最低要求

每个迁移 PR 必须包含:

- 影响子系统
- 关联对标条目
- 关联 ADR 或差异 ID
- 测试变化
- 状态文档变化

未满足以上条件的 PR，不视为完整迁移 PR。

## 5. 状态流转规则

### 对标状态

- `Not Started` -> `Spec Ready`
  - 条件: 有边界说明、Rust 归属、初步验收标准
- `Spec Ready` -> `In Progress`
  - 条件: 开始实现
- `In Progress` -> `Parity Pending`
  - 条件: 代码基本完成，等待对标验证
- `Parity Pending` -> `Parity Verified`
  - 条件: 测试、平台验证、差异登记全部完成
- 任意状态 -> `Accepted Difference`
  - 条件: 存在经批准的行为偏离，且已完成差异流程

### ADR 状态

- `Proposed` -> `Accepted`
- `Accepted` -> `Superseded`
- `Proposed` -> `Rejected`

## 6. 紧急变更

没有“先做再补文档”的紧急通道。若确有阻塞，至少要先落一份最小 ADR 或差异登记，再继续实施。
