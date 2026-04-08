# ADR 索引

本目录用于记录 Clawin 的架构级硬决策。凡是会影响跨子系统实现方式、评审口径或对标策略的事项，都必须通过 ADR 固化。

## 状态

- `Proposed`: 已提出，尚未生效
- `Accepted`: 已生效，后续实现必须遵循
- `Superseded`: 被后续 ADR 替代
- `Rejected`: 明确不采纳

## 当前 ADR 列表

| 编号 | 标题 | 状态 |
| --- | --- | --- |
| `ADR-0001` | Workspace 与 crate 边界 | `Accepted` |
| `ADR-0002` | Runtime 与状态模型 | `Accepted` |
| `ADR-0003` | TUI 技术路线 | `Accepted` |
| `ADR-0004` | 平台抽象边界 | `Accepted` |
| `ADR-0005` | 配置持久化与迁移策略 | `Accepted` |
| `ADR-0006` | Feature gating 策略 | `Accepted` |

## 编写规则

- 每个 ADR 只回答一个决策问题
- 必须写清楚背景、决策、后果
- 若推翻旧决策，新 ADR 必须显式标记被替代文档
- 若 ADR 引入行为偏离，必须分配差异 ID
