# Clawin 主路线图

- 状态: Active
- 基线日期: 2026-04-08

## 总目标

以 Rust 重构 Claude Code `2.1.88` 公开包能力，形成可发布、可验证、可扩展的 `clawin`。

## 阶段划分

### Phase 0: 文档与治理基线

- 目标
  - 建立宪章、对标矩阵、ADR、工程标准、路线图、状态文档
- 入口条件
  - 仓库存在上游 sourcemap 参考语料
- 退出条件
  - 文档体系完整，可作为后续实施基线

### Phase 1: Workspace 与基础装配

- 目标
  - 建立 Cargo workspace、基础 crate、CI 与 lint/test 骨架
- 入口条件
  - Phase 0 完成
- 退出条件
  - 能构建空骨架，基础依赖与 crate 边界稳定

### Phase 2: Config / Bootstrap

- 目标
  - 实现 `clawin-bootstrap`、`clawin-config` 的首轮能力
- 范围
  - 启动、路径归一化、全局/项目配置、schema version、migration 框架
- 退出条件
  - bootstrap/config 子系统达到可测试、可演示、可继续扩展的状态

### Phase 3: Tools / Commands 基础设施

- 目标
  - 建立 tool registry、command registry、permission 基础链路
- 退出条件
  - 能在无完整 engine 的前提下跑通命令和工具骨架

### Phase 4: Conversation Engine

- 目标
  - 实现 turn loop、streaming、tool orchestration、compact、budget
- 退出条件
  - 能跑通最小端到端会话

### Phase 5: TUI / REPL

- 目标
  - 重建终端交互体验
- 退出条件
  - REPL 基本可用，交互/非交互路径分离稳定

### Phase 6: MCP / Skills / Plugins

- 目标
  - 补齐外部协议能力和内容加载机制
- 退出条件
  - MCP 基础能力、skills/plugins 加载规则可用

### Phase 7: Worktree / Session / Resume / Remote 收口

- 目标
  - 收口会话恢复、worktree、structured IO、remote 相关能力
- 退出条件
  - 关键协作流程具备端到端行为闭环

### Phase 8: Parity Hardening 与发布准备

- 目标
  - 完成 golden fixture、三平台矩阵、差异审查、发布检查表
- 退出条件
  - 满足 V1 发布门槛

## 里程碑

| 里程碑 | 定义 |
| --- | --- |
| `M0` | 文档治理体系建立 |
| `M1` | Rust workspace 可构建 |
| `M2` | bootstrap/config 通过首轮试运行 |
| `M3` | commands/tools/engine 能跑通最小会话 |
| `M4` | TUI 与 MCP 主链路可用 |
| `M5` | 三平台对标验证完成 |

## 关键依赖

- 上游公开包行为样本与 fixture 收集
- 三平台持续集成环境
- 可稳定执行的 golden fixture 测试框架
- git/worktree/terminal 相关平台验证环境

## 当前建议迁移顺序

1. `config` + `bootstrap`
2. `tools` + `commands`
3. `engine`
4. `ui`
5. `mcp` + `skills/plugins`
6. `worktree` + `session/resume` + `remote`

## 风险门槛

若出现以下情况，必须暂停扩展实现并优先治理:

- crate 边界开始失真
- 对标矩阵无法支持评审
- 差异记录失控
- Windows 平台长期无人验证
- 核心行为只能靠人工口头解释
