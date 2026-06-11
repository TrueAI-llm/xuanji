# Role 执行接线设计（接通 God Role 编排闭环）

> **状态**：待评审
> **日期**：2026-06-11
> **母设计**：[`2026-06-11-role-self-evolution-design.md`](./2026-06-11-role-self-evolution-design.md)
> **目标**：补完母设计中"Role 持有真实 Agent 并执行"这一被当前实现遗漏的执行接线，让 God Role 编排闭环真正运转（含 LLM 编排、自动 hire、分级安全 fire）。

---

## 一、背景：母设计意图 vs 当前实现

母设计（`2026-06-11-role-self-evolution-design.md`）定义了完整的 Role 自进化系统：Role 持有 Agent 执行子任务、God Role 作为默认入口编排、知识通过 RoleStore + TeachingLibrary 持久化与传播。

当前实现（commits `b99603b`→`ffa349f`）建好了**元数据与编排骨架**——`xuanji-role` crate 的类型/持久化/反思/发现/教学库、`Orchestrator` 的 decompose/match/fire-suggest、CLI 的 `role hire/fire/list/show/activate/chat/evolve`、God Role 默认入口。**代码能编译**。

但执行层**没接上**，母设计要求的几处接线在实现中被省略，导致系统**只搬运元数据、不执行任何真实任务**：

| 母设计要求 | 母设计位置 | 当前实现 | 后果 |
|---|---|---|---|
| `run_prompt(&prompt, &config)` 传入配置 | 母设计 §六 L549-571 | `commands::god::run_prompt(&prompt)` **未传 config** | god.rs 无法构造 provider，无 LLM |
| Role 持有 Agent 并 `agent.run()` | 母设计 §一 L84, §二 L155-161 | `Role.agent` 在所有 CLI 路径恒为 `None` | 不发起任何 LLM 调用 |
| bootstrap God 时 `.with_stage(Expert).with_role_tools(...)` | 母设计 §六 L575-586 | 仅 `Role::new + activate` | God 无 role.* 工具、阶段为 Seed |
| persona / self_description 影响行为 | 母设计 §一（self_description 自行演化） | 从不进 `build_system_prompt` | 不同角色行为完全相同 |
| LLM 驱动 REFLECT/DISCOVER/DECOMPOSE | 母设计 §二 各阶段 | 关键词启发式（`SKILL_KEYWORDS`、`match_roles` 写死分支） | 编排/发现是假的 |

编译器已实锤：`agent.rs` 的 `run_agent` / `run_chat` / `create_provider` / `create_registry`（真正构造 provider、注册工具、跑 agent 循环的函数）全是 `never used`。

## 二、目标与非目标

### 目标（本期）
1. Role（含 God Role 与被调度的 worker）持有**真实 Agent**并真正执行 LLM + 工具调用。
2. 角色**专长注入**系统提示词，使不同角色行为不同（兑现"每个角色专门精通一些领域"）。
3. **记忆统一为 RoleStore**（退役角色路径上的 LongTermMemory），角色知识真正流入提示词、执行结果真正回流学习。
4. God Role 编排闭环**LLM 驱动**：decompose → match → dispatch → aggregate。
5. **自动 hire**：发现技能缺口时当场创建专家角色并派发任务。
6. **分级安全 fire**：空超期角色自动归档、有数据角色仅建议。
7. chat 模式走**统一编排路径**。
8. 修复实现 bug（空 purpose 覆盖、假执行统计等）。

### 非目标（后续）
- 工具白名单过滤（per-role tool whitelist）
- 角色 × 项目二维记忆分桶（本期纯角色维度）
- daemon/cron 调度的自驱循环、Sentinel 守护（母设计 §四的运行时模型）
- `role.pause/resume/step`、`teaching subscribe`、`sessions/` 执行记录
- 单独配置"路由模型"（`router_provider`）；本期复用 default provider
- hire/fire 的 UI 确认流程（本期 hire 全自动、fire 按 §七分级）

---

## 三、核心闭环

```
用户输入
  │
  ▼
GodRole.run_orchestrated_cycle(goal)
  │
  ├─ ① LLM decompose:  goal  →  SubTask[]{ description, needed_expertise }
  ├─ ② LLM match:      每个 SubTask + 可用角色清单(name+purpose)
  │                     → assignee=<role> | HireSignal{purpose}
  ├─ ③ dispatch:
  │     • 已分配角色  → 该角色持真实 Agent 执行（persona + render_context + 工具）
  │     • HireSignal  → 自动 hire 新角色 → 立即执行同一子任务
  ├─ ④ LLM aggregate:  各子结果 → 最终回答（markdown 渲染返回用户）
  └─ ⑤ reflect:        每个角色（含 god）对自己的子任务结果学习 → persist
```

每个被调度的角色执行时，构成**角色级微闭环**：

```
Role.render_context()  →  字符串（rules + 相关 cases + teachings + context 笔记）
        │
        ▼
build_system_prompt(persona=该角色, memory_context=↑)
        │
        ▼
Agent.run()  →  ExecutionStats{ text, tool_calls, tokens, success }
        │
        ▼
Role.reflect(stats)  →  更新 rules/cases/preferences → persist
```

**关键架构决策**：Agent **不持有**记忆对象，而是接收「渲染好的上下文字符串」并返回「执行统计」。理由：
- 复用 `build_system_prompt` 已有的 `memory_context: Option<&str>` 接缝，零额外抽象。
- 避免 `xuanji-role ↔ xuanji-agent` 的 crate 循环依赖（Role 持有 Agent，若 Agent 再持有 RoleStore 则成环）。
- 避免 async run loop 中的 `&mut` 记忆所有权问题。

（若将来需要更强封装，可将一个 `RoleMemory` trait + 无关 `OutcomeRecord` 类型放进 `xuanji-agent` 来打破循环；本期 YAGNI，走字符串方案。）

---

## 四、改动清单

### C1 — 角色专长注入系统提示词
**文件**：`crates/xuanji-agent/src/prompt.rs`、`crates/xuanji-agent/src/agent.rs`

- `build_system_prompt(...)` 增加参数 `persona: Option<&str>`，置于现有硬编码 "你是 xuanji…" 段之前；persona 存在时以其为主人设。
- `Agent` 增加字段 `persona: Option<String>` + 构建器 `with_persona(&str)`；`Agent::run` 构造提示词时传入。
- persona 由 `RoleProfile` 渲染：`self_description` + `seed_purpose` + `evolution_stage`（例："你是 {self_description}（{stage}）。专精方向：{seed_purpose}。"）。新增 `Role::render_persona() -> String`。
- 验收：两个 purpose 不同的角色，生成的 system prompt 含各自描述。

### C2 — Role 接真实 Agent（最核心）
**文件**：`crates/xuanji-cli/src/commands/runtime.rs`（新建，从 `agent.rs` 抽取）、`god.rs`、`role.rs`、`main.rs`

- 将 `agent.rs` 现有 dead 的 `create_provider` / `create_registry` 提到共享模块 `commands/runtime.rs`，供 `god.rs`、`role.rs` 复用（`agent.rs` 旧入口可保留或删除）。**同时把 `swarm.rs` / `workflow.rs` 各自私有的同名副本一并迁入并去重**，彻底消除 dead/dup 代码。
- **复用 `xuanji-core` 已有的 `register_system_tools` / `register_shell_run` / `register_agent_delegate`**（`xuanji-core/src/system_tools.rs:67/91/108`），不要另起炉灶注册 shell.run；未来的 `role.*` 系统工具也走同一 `register_system_tool` 机制。
- 新增构造器 `Role::with_real_agent(provider, registry, agent_config)`（或在外部构造 `Agent` 后用现有 `with_agent`），内部完成：注册 `shell.run` + MCP 工具 + `with_persona(render_persona())` + 注入 `render_context()`。
- **`main.rs` 改为传入 config**：
  - `commands::god::run_prompt(&prompt, &config).await`
  - `commands::god::run_chat(&config).await`
  - `commands::role::handle_role(&action, &config).await`
- `god.rs::bootstrap_god(&config)`：构造 god 的真实 Agent；按母设计设 `Stage::Expert`（见 C8 bug 表）。
- `run_orchestrated_cycle` 中派发给 worker 时，worker 也持真实 Agent（见 C5）。
- 验收：`cargo check` 后 `agent.rs` 的四个函数不再是 dead code；`xuanji "你好"` 真实发起一次 LLM 调用并返回回答。

### C3 — 记忆统一为 RoleStore
**文件**：`crates/xuanji-role/src/{store,lib,types}.rs`、`crates/xuanji-agent/src/agent.rs`、`crates/xuanji-cli/src/commands/memory.rs`

- RoleStore 成为角色路径上**唯一**持久化记忆，纯角色维度分桶（`~/.xuanji/roles/<role>/`）。
- **新增自由文本上下文字段**（承载原 LongTermMemory 的 ProjectContext 用途）：
  - `RoleStore` 增加 `context/context.json`（结构：`{ notes: String, focus: String }` 或简单 `String`）+ `save_context` / `load_context`。
- **新增 `Role::render_context() -> String`**：拼装 高置信 rules + 相关 cases（按 context_tags）+ 相关 teachings（query_by_tags）+ context 笔记，渲染为提示词块（复用 `LongTermMemory::to_prompt_context` 的 Markdown 段风格）。
- **退役角色路径上的 LongTermMemory**：
  - `Agent` 不再在 `load_memory_context()` 中自建 LTM、不再用 `current_dir()` 分桶；改为接受外部传入的 memory_context 字符串（由 Role 提供）。
  - 保留 `xuanji-memory` crate 与 `LongTermMemory` 类型本身（非角色路径/未来 role×project 维度可能复用），仅在 God Role 入口停用。
- **`xuanji memory show/clear/rule` 改指向当前活跃角色**（默认 god）的 RoleStore：
  - `show` → `Role::render_context()`
  - `clear` → 清该角色 cases/rules/preferences（不动 profile）
  - `rule <text>` → 追加为该角色的一条低置信 Rule（confidence 0.5）
- 验收：角色执行后磁盘上 `~/.xuanji/roles/<role>/learning/cases.json` 增长；下次执行 `render_context()` 能读回该知识。

### C4 — 执行统计回流（修掉假数据）
**文件**：`crates/xuanji-agent/src/agent.rs`、`crates/xuanji-role/src/lib.rs`

- `Agent::run()` 返回 `ExecutionStats { text: String, tool_calls: u32, tokens: u32, success: bool }`（在现有 `String` 基础上扩展；run loop 中累计工具调用次数，token 由 provider 响应累计或估算）。
- `Role` 将 `ExecutionStats` 转为 `GoalOutcome`（真实 `tool_calls_count` / `tokens_used` / `success`），再喂 `LearningEngine::reflect_on_goal`。
- 验收：一次执行后产生的 CaseEntry 的 `tool_calls_count > 0`、非异常时 `tokens_used` 非零。

### C5 — LLM 驱动编排（decompose / match / aggregate）
**文件**：`crates/xuanji-role/src/{lib,discover}.rs`（新增 `orchestrate.rs` 或扩展 `discover.rs`）

- 新增 `Orchestrator` 的 LLM 版本（复用 default provider），三次结构化输出调用：
  1. **decompose**：输入 goal → 输出 `Vec<SubTask>{ description, needed_expertise }`（JSON schema 约束）。
  2. **match**：输入 SubTask 列表 + 可用角色清单（`RoleStore::list_roles()` + 各 `seed_purpose`）→ 输出每任务的 `Assignment{ task_idx, assignee: Option<role_name>, hire: Option<purpose> }`。
     - **不新增并行表示**：`Assignment` 是 match 的内部结构；其 `hire` 信号转换为现有的 `OrchestrationSuggestion{ kind: HireRole, ... }` 进入 `CycleResult` / CLI 打印路径（C6 消费 hire，C7 消费 fire 建议），保持单一建议表示。
  3. **aggregate**：输入各子任务结果 → 输出最终回答（纯文本/markdown）。
- 现有关键词启发式（`DiscoverEngine::decompose`、`Orchestrator::match_roles` 写死 skill 分支、`SKILL_KEYWORDS`、`skill_translate`）**降级为 fallback**：仅当 LLM 调用失败时使用。
- dispatch：已分配任务交对应角色（持真实 Agent）执行；`hire` 字段触发 C6。
- 编排受 `xuanji-budget` 约束：复用 `BudgetController` + `max_depth` 限制拆解深度与并发开销。
- 验收：给一个明显跨领域任务（如"审计安全漏洞和优化性能"），LLM 拆出 ≥2 子任务并分别派发；无匹配角色时产出 hire 信号。

### C6 — 自动 Hire
**文件**：`crates/xuanji-role/src/lib.rs`、`crates/xuanji-cli/src/commands/god.rs`

- `match` 阶段产出 `hire: Some(purpose)` 时，God Role **当场**：
  1. 由 purpose 派生 `name`（slug；可用一次轻量 LLM 调用生成精炼 name+persona）。
  2. `Role::new(name, purpose)` 创建 → 持真实 Agent（同 C2）。
  3. 立即 dispatch 触发该 hire 的子任务 → 新角色从第一笔执行开始积累 cases/rules。
  4. hire 事件写入 god 的 outcomes/日志，`xuanji role list` 可见新角色。
- 默认全自动。配置开关 `[role] auto_hire = true`（默认 true），关闭时降级为"只打印 hire 建议"。
- 验收：对无匹配角色的任务，执行后 `xuanji role list` 出现新角色且其 `cases.json` 非空。

### C7 — 分级安全 Fire（方案 a）
**文件**：`crates/xuanji-role/src/{store,lib}.rs`、`crates/xuanji-cli/src/commands/{role,god}.rs`

- **`RoleStore::delete` 改为 `archive`**：将角色目录移动到 `~/.xuanji/roles/.archived/<name>-<timestamp>/`，不再 `remove_dir_all` 硬删；保留恢复路径。新增 `RoleStore::restore`。
- God Role 在编排周期开始时评估现有角色，分级处理：
  - **注意**：`is_stale` 当前解析 `%Y-%m-%d %H:%M:%S` 且解析失败静默返回 `false`（`lib.rs:112-120`）；归档/恢复不得改动 profile 的日期格式，否则过期判定失效。`is_stale(_, 7)` 的硬编码阈值改为读取 `[role] fire_stale_days` 配置（见 §五），否则配置项形同虚设。

  | 角色状态 | 处理 |
  |---|---|
  | `Stage::Seed` + 0 cases + 创建超期（>7 天，`is_stale`） | **自动归档**（archive） |
  | 有 cases/rules 但长期低成功率或疑似冗余 | 仅生成 `FireRole` 建议打印 |
  | 需要 `RedefinePurpose` | 仅生成建议 |

- `xuanji role fire <name>` CLI 同样改为归档而非硬删（除非加 `--purge`）。
- God Role 不可被 fire（现有保护保留）。
- 验收：创建一个空角色、改其 `created_at` 为 >7 天前、跑一次 god 周期 → 该角色出现在 `.archived/`；有 cases 的角色只产出建议不被移动。

### C8 — chat 统一编排
**文件**：`crates/xuanji-cli/src/commands/god.rs`

- `run_chat(&config)`：复用 `agent.rs` 现有聊天循环机制（`enable_chat_mode` + `ShortTermMemory`）保留对话上下文，但**每轮用户输入走 `run_orchestrated_cycle`**（decompose→…→aggregate）。
- chat 模式下 God Role 持久持有同一个 Agent（含 chat memory）跨轮复用；worker 角色 per-turn 构造。
- 现有 `/roles`、`/teachings`、`/help`、`/quit` 斜杠命令保留。
- 验收：`xuanji chat` 中输入跨领域任务，可见编排过程（派发到的角色），最终返回聚合回答。

### C9 — Bug 修复
| Bug | 位置 | 修复 |
|---|---|---|
| worker 用空 purpose 重建覆盖已有角色 | `lib.rs` `run_orchestrated_cycle` 中 `Role::new(assignee, "")` | 先 `RoleStore::load_profile`，存在则复用其 purpose，不存在才用 match 阶段的建议 purpose 创建 |
| `bootstrap_god` 未设 Expert / 未带 role 工具 | `god.rs` | 按 C2 设 `Stage::Expert`（母设计 §六） |
| `tokens_used` 恒 0、`tool_calls_count` 失真 | `lib.rs` 各处 | C4 真实统计 |
| dead code（`run_agent` 等） | `agent.rs` | C2 复用后自然消除 |

---

## 五、数据模型变更

### RoleStore 新增
- `context/context.json` —— 角色 自由文本上下文（notes / focus），替代 LTM 的 ProjectContext。
- `~/.xuanji/roles/.archived/<name>-<ts>/` —— 归档目录（C7）。

### 类型新增/调整
- `xuanji-agent`：`ExecutionStats { text, tool_calls, tokens, success }`；`Agent::persona: Option<String>` + `with_persona`。
- `xuanji-role`：`Role::render_persona()`、`Role::render_context()`、`Role::with_real_agent(...)`；`Orchestrator` LLM 版本 + 结构化 `Assignment` 类型；`RoleStore::archive/restore/context`。
- `xuanji-agent::prompt::build_system_prompt(..., persona: Option<&str>)`。

### 配置新增（`~/.xuanji/config.toml`）
```toml
[role]
auto_hire = true          # 默认自动 hire；false 时仅建议
fire_stale_days = 7       # 空 Seed 角色自动归档阈值
```
（`XuanjiConfig` 增加 `role: RoleCliConfig` 段，`#[serde(default)]`。）

---

## 六、Crate 依赖与循环规避

- 现状：`xuanji-role → xuanji-agent`（Role 持有 Agent）。**禁止**反向依赖。
- 因此：Agent **不**持有 `RoleStore`，记忆以字符串注入（C3）。`ExecutionStats` 定义在 `xuanji-agent`（低层），`xuanji-role` 消费它 → 单向依赖，无环。
- `RoleStore`、`LearningEngine`、`Orchestrator`（LLM 版）均在 `xuanji-role` 内；它们需要的 provider 通过参数注入，不在 `xuanji-role` 内部持有 provider 句柄。

---

## 七、测试策略

### 单元测试
- `render_persona()`：不同 profile 产出含各自描述的字符串。
- `render_context()`：注入 rules/cases/teachings 后输出含其内容；空记忆时输出合理空态。
- `build_system_prompt` persona 注入：persona 存在/不存在两条分支。
- `RoleStore::archive/restore`：归档后 `list_roles` 不含、`.archived/` 含；restore 后回归。
- `Orchestrator::match`（fallback 路径）保留现有测试；LLM 路径用 mock provider。
- bug 回归：worker 重建不覆盖已有 purpose。

### 集成测试（"接通了"的硬证据）
- mock provider 注入 → `run_prompt("简单任务", &config)`：
  - 断言发起了 ≥1 次 LLM 调用；
  - 断言 god 的 `cases.json` 新增 ≥1 条、`tool_calls_count > 0`；
  - 断言返回非空文本。
- mock provider → 跨领域任务 → 断言拆出 ≥2 子任务、分别派发、aggregate 返回。
- hire：无匹配角色任务 → 断言新角色目录出现、其 cases 非空。
- fire：伪造超期空角色 → 断言被归档；有 cases 角色 → 断言仅产建议。

### 手动验收
- `xuanji "列出当前角色"` 真实返回 God Role 的回答（非空操作）。
- `xuanji chat` 多轮对话上下文保持。
- `xuanji role hire frontend --purpose "前端工程"` 后 `xuanji role show frontend` 正常。

---

## 八、分期里程碑

| 里程碑 | 内容 | 价值 |
|---|---|---|
| **M1 接通** | C1+C2+C3+C4+C9 | God Role 真正能执行单任务并学习；最小可跑 |
| **M2 编排** | C5 | LLM decompose/match/aggregate，已有角色间调度 |
| **M3 自治** | C6+C7 | 自动 hire + 分级 fire；生态自生长 |
| **M4 体验** | C8 + 配置段 | chat 统一编排、配置开关 |

建议按 M1→M4 顺序交付，每个里程碑独立可验证、可提交。

---

## 九、风险与缓解

| 风险 | 缓解 |
|---|---|
| 编排多轮 LLM 调用 token 成本高 | `BudgetController` + `max_depth` 约束；chat 简单输入可走 god 直答（decompose 产出单任务时不展开） |
| 自动 hire 产生垃圾角色 | 默认 auto_hire 但 fire 机制（C7）会归档空超期角色自清理；name 由 purpose 派生避免重名 |
| 自动 fire 误删有价值角色 | 方案 a：有数据的角色永不自动 fire，仅建议；空角色也只归档不硬删，可 restore |
| LLM 结构化输出不稳定 | JSON schema 约束 + 失败回退到关键词启发式 fallback |
| 退役 LTM 影响现有 `xuanji memory` 用户 | memory 命令重定向到角色 RoleStore（C3），语义保持（show/clear/rule） |
| `xuanji-memory` crate 是否要彻底移除 | 本期不移除，仅停用角色路径上的使用；保留类型供未来 role×project 维度复用 |

---

## 十、与母设计的关系

本 spec **不修改**母设计的数据模型（`RoleProfile`/`Stage`/`GoalNode`/`Rule`/`CaseEntry`/`Teaching`/`RoleStore` 结构）、自驱循环六阶段（REFLECT/DISCOVER/PRIORITIZE/DECOMPOSE/EXECUTE/LEARN）、TeachingLibrary 传播机制、God Role 定位。这些已在当前实现中落地且经测试。

本 spec **仅补完**母设计明确要求但实现遗漏的**执行接线**（C1-C4）、将**启发式编排升级为 LLM 驱动**（母设计 §二本就要求 LLM 调用，C5）、并**落地 hire/fire**（母设计 §五 God Role 能力 + §四 CLI 的 role hire/fire，C6-C7）。实现完成后，当前代码即成为母设计的**完整**实现。
