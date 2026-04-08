# 测试与对标规范

- 状态: Accepted
- 适用范围: 所有子系统迁移与发布验收

## 1. 测试目标

Clawin 的测试不是“证明 Rust 代码能运行”，而是“证明它与 Claude Code `2.1.88` 公开行为一致，或存在已批准差异”。

## 2. 测试分层

### 2.1 单元测试

- 纯函数、解析器、状态转换、路径规则、权限判断
- 必须快速、稳定、无外部依赖

### 2.2 集成测试

- crate 之间的契约测试
- engine + tools
- bootstrap + config
- commands + engine
- config + persistence + migration

### 2.3 Golden Fixture 测试

- CLI help 和错误输出
- slash command 输出
- transcript / streaming 事件序列
- tool schema 与 tool result 结构
- resume / session 恢复序列

### 2.4 三平台测试

- macOS
- Linux
- Windows

至少覆盖:

- 路径处理
- shell 执行
- TTY 能力
- 换行与编码
- secure storage
- 进程与中断

## 3. 对标验证规则

每个子系统进入 `Parity Pending` 前，必须具备:

- 明确的上游入口
- 对应的 Rust 实现归属
- 黄金路径测试
- 失败路径测试
- 平台相关风险说明

每个子系统进入 `Parity Verified` 前，必须额外具备:

- golden fixture 或等价行为快照
- 三平台相关测试结果或明确豁免说明
- 差异 ID 全量登记

## 4. Fixture 来源

对标 fixture 的主要来源:

- `docs/claude-code-sourcemap-main/package/`
- `docs/claude-code-sourcemap-main/restored-src/src/`
- 通过公开包实际运行得到的行为样本

所有 fixture 都必须标注来源和版本。

## 5. 差异处理

- 没有差异 ID 的行为变化，默认视为 bug
- `Accepted Difference` 不能替代测试
- 即使是被接受的差异，也必须有回归测试来锁定新行为

## 6. PR 验收要求

每个迁移 PR 至少附带:

- 影响的对标条目
- 新增或更新的测试
- 是否引入差异 ID
- 三平台影响说明
- 风险与后续项

## 7. Bootstrap / Config 试运行模板

以 `bootstrap/config` 为首轮试运行对象时，至少需要验证:

- 启动时 session/runtime 的基本装配
- 全局配置与项目配置读取
- 项目根判定规则
- schema version 与 migration 框架
- `DIFF-2026-001` 是否在测试和文档中被正确引用
