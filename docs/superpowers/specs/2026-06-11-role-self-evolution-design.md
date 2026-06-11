# Role 自进化系统设计

> **状态**：设计中
> **日期**：2026-06-11
> **目标**：让 xuanji 支持创建自主、自驱、自进化的 Role，并通过 God Role 统一统筹入口

---

## 一、架构与数据模型

### 新增 Crate：`xuanji-role`

在当前 9 个 crate 之上新增一层，负责 Role 的自治生命周期管理。

```
xuanji-role          (新增 —— Role 自主生命周期)
├── xuanji-agent     (复用 —— 执行具体任务)
├── xuanji-memory    (复用 —— Role 专属长期记忆)
├── xuanji-bus       (复用 —— Role 间发现传播)
├── xuanji-budget    (复用 —— 每 Role 独立预算)
├── xuanji-llm       (复用 —— 反思/发现/学习)
└── xuanji-plugin    (复用 —— 工具注册表)
```

### Role 核心数据结构

```rust
// crates/xuanji-role/src/types.rs

pub struct RoleProfile {
    pub name: String,
    pub seed_purpose: String,         // Hire 时设定的初始方向
    pub self_description: String,     // Role 对自身的认知（会自行演化）
    pub created_at: String,
    pub evolution_stage: Stage,       // 进化阶段
}

pub enum Stage {
    Seed,           // 刚被 hire，还没积累经验
    Exploring,      // 主动探索方向，积累案例
    Specializing,   // 已形成专长领域
    Expert,         // 在特定领域高度自主
}

pub struct GoalNode {
    pub id: String,
    pub description: String,
    pub priority: f32,                // 0.0 ~ 1.0
    pub parent_id: Option<String>,    // 从哪个目标拆解来的
    pub status: GoalStatus,
    pub created_by: GoalSource,       // User / SelfDiscovered / Derived
}

pub struct Rule {
    pub id: String,
    pub condition: String,            // "当遇到 X 类型的任务时"
    pub action: String,               // "优先使用 Y 策略"
    pub confidence: f32,              // 置信度，随验证调整
    pub source_case_id: Option<String>,
    pub validated_count: u32,         // 验证次数
}

pub struct CaseEntry {
    pub id: String,
    pub task_description: String,
    pub context_tags: Vec<String>,     // 用于语义检索
    pub strategy_used: String,         // 采用的方法
    pub outcome: CaseOutcome,
    pub lessons: String,               // LLM 反思总结
}

pub struct ToolPreference {
    pub tool_name: String,
    pub success_rate: f32,
    pub avg_token_cost: u32,
    pub preferred_scenarios: Vec<String>,
}
```

### 与现有架构的关系

| 现有组件 | Role 如何使用 |
|---------|-------------|
| `xuanji-agent::Agent` | Role 内部持有一个 Agent，用于执行分解出来的子任务 |
| `xuanji-memory::LongTermMemory` | 升级为 Role 专用，存储 Role 级知识而非项目级 |
| `xuanji-bus::KnowledgeBus` | Role 间通过 bus 分享发现和洞察 |
| `xuanji-budget::BudgetController` | 每个 Role 独立预算配额的复合控制器 |
| `xuanji-plugin::ToolRegistry` | Role 自行决定需要哪些 MCP 工具，可自扩展 |

---

## 二、自驱循环

Role 的核心——一套在 Agent ReAct 循环之上的元认知循环，让 Role 自主决定 "该做什么"。

### 自驱循环结构

```
┌─────────────────────────────────────────────────────────────┐
│                    Role::run_cycle()                         │
│                                                              │
│  ┌──────────┐    ┌──────────┐    ┌──────────┐               │
│  │ REFLECT  │───▶│ DISCOVER │───▶│ PRIORITIZE│              │
│  │ 回顾学习  │    │ 发现机会  │    │ 目标排序  │              │
│  └──────────┘    └──────────┘    └─────┬────┘              │
│        ▲                               │                     │
│        │                          ┌────▼────┐               │
│        │                          │ DECOMPOSE│              │
│        │                          │  拆解任务  │              │
│        │                          └────┬─────┘              │
│        │                               │                     │
│  ┌─────┴─────┐                  ┌──────▼──────┐            │
│  │   LEARN   │◀─────────────────│   EXECUTE   │            │
│  │   学习巩固  │                  │ Agent执行    │            │
│  └───────────┘                  └─────────────┘            │
│                                                              │
│  [循环间隔: 完成一个目标 → 进入下一轮]                          │
│  [持久化: 每轮结束保存状态到 RoleStore]                        │
└─────────────────────────────────────────────────────────────┘
```

### 各阶段详细逻辑

**1. REFLECT（回顾）** —— 回顾已完成的目标

- 输入：上一次执行的结果、goal 状态、CaseOutcome
- LLM 调用：以当前 Role 视角，反思 "学到了什么"
- 产出：更新 self_description、生成新 Rule、归档 Case

**2. DISCOVER（发现）** —— 寻找下一步值得做的事

- 输入：seed_purpose + self_description + 已学知识 + 环境观察（项目状态、bus 消息）
- LLM 调用：生成候选目标列表，每个候选附带 "为什么值得做" 的推理
- 产出：`Vec<GoalCandidate>`
- 关键约束：候选目标必须与 seed_purpose 有语义相关性（防止 Role 偏离方向）

**3. PRIORITIZE（排序）** —— 对候选目标打分排序

信号叠加权重：
- 与 purpose 的语义相似度（向量或 LLM 打分）
- 该方向已有 case 的成功率
- 该方向的探索度（避免重复）
- token 预算剩余量

产出：Top-1 目标作为本轮执行目标

**4. DECOMPOSE（拆解）** —— 将目标分解为可执行子任务

- LLM 调用：基于 CaseBase 中类似案例的拆分模式
- 产出：子任务列表 `Vec<SubTask>`，可选依赖关系
- 每个 SubTask 可直接由 Agent.run() 执行

**5. EXECUTE（执行）** —— 复用现有 Agent

```rust
for subtask in subtasks {
    let result = self.agent.run(subtask.description).await?;
    // 在执行过程中，tool preference 在运行态实时调整
    subtask.result = result;
}
```

**6. LEARN（学习）** —— 从执行结果中提取知识

- 触发多机制学习
- 更新 Rules、Cases、ToolPreferences
- 调整 evolution_stage

### 循环触发与暂停

| 模式 | 行为 |
|------|------|
| `role activate` | 启动循环，持续运行直到用户停止或目标队列为空 |
| `role step` | 执行一轮（REFLECT → DISCOVER → ... → LEARN），适合调试 |
| Daemon 集成 | Role 在 daemon 中按 cron 节奏运行 |
| Budget 熔断 | 超出预算时暂停，等待用户确认或充值 |

---

## 三、学习机制与知识传播

### 三层知识模型

```
┌─────────────────────────────────────────────────────────────┐
│                      LearningStore                           │
│                                                              │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐  │
│  │   Rules      │  │   Cases     │  │  ToolPreferences    │  │
│  │ 行为规则      │  │ 案例模式     │  │  工具偏好信号        │  │
│  │ if→then      │  │ task→soln   │  │  success_rate/场景  │  │
│  └──────┬──────┘  └──────┬──────┘  └──────────┬──────────┘  │
│         │                │                     │              │
│         └────────────────┼─────────────────────┘              │
│                          ▼                                    │
│              ┌──────────────────────┐                        │
│              │       Teachings      │  ◀── 发布到教学库       │
│              │    (经验证的知识包)     │                        │
│              │  - 可被其他 Role 订阅   │                        │
│              │  - 附验证次数和置信度    │                        │
│              └──────────────────────┘                        │
└─────────────────────────────────────────────────────────────┘
```

### 学习触发时机

| 时机 | 触发什么 |
|------|---------|
| 每个子任务完成后 | 更新 ToolPreferences（成功/失败信号） |
| 每个 Goal 完成后 | 生成 CaseEntry + LLM 反思 → 提炼 Rule |
| Rule 验证通过 N 次 | 提升置信度，标记为 `teachable` |
| Role 主动触发 | `role evolve <name>` 深度复盘 |

### 知识传播：Teaching

```rust
pub struct Teaching {
    pub id: String,
    pub author_role: String,
    pub kind: TeachingKind,
    pub content: String,           // 人类/LLM 可读的描述
    pub confidence: f32,           // 0.0 ~ 1.0，随验证调整
    pub validation_count: u32,
    pub domain_tags: Vec<String>,  // 用于跨 Role 检索匹配
    pub created_at: String,
}

pub enum TeachingKind {
    Rule,          // "When doing X, prefer Y approach"
    AntiPattern,   // "Avoid doing Z because ..."
    Heuristic,     // "In domain D, tool A > tool B"
    CaseStudy,     // 完整的 case 示例
}
```

### 传播路径

```
Role-A                          Role-B
  │                               │
  │  执行 Goal → 学到 Rule          │  面临相似任务
  │  验证 3 次，置信度 0.85          │  查询 TeachingLibrary
  │  → publish(teaching)    ────▶  │  匹配到 Role-A 的 teaching
  │                               │  应用 Rule → 成功
  │  ◀── 反馈: B 验证成功 ────────   │  → validate(teaching_id)
  │  置信度 +1，提升至 0.92          │
  │                               │
  ▼                               ▼
       TeachingLibrary (共享)
```

### TeachingLibrary 能力

| 操作 | 说明 |
|------|------|
| `publish(teaching)` | Role 发布经自己验证的知识 |
| `query(domain, context)` | 检索相关 teachings（按 domain_tags + 语义匹配） |
| `validate(id, success)` | 消费方验证后反馈，双向提升置信度 |
| `subscribe(role_name)` | 订阅某 Role 的 teachings |
| `list()` | 浏览知识库 |

### CLI 命令

```bash
xuanji role teach <name>         # 手动让 role 生成 teaching
xuanji teaching list             # 浏览知识库
xuanji teaching show <id>        # 查看详情
xuanji teaching subscribe <role> # 订阅某 role 的知识
```

---

## 四、持久化与运行时

### RoleStore 目录结构

```
~/.xuanji/roles/<role-name>/
├── profile.toml          # 不可变身份 + 可演化 self_description
├── state.json            # 当前 evolution_stage、活跃状态
├── queue/
│   ├── goals.json        # 目标队列（GoalNode 列表）
│   └── archive/          # 已完成/放弃的目标归档
│       └── 2026-06-11/
│           └── goal-001.json
├── learning/
│   ├── rules.json        # Rule 列表
│   ├── cases.json        # CaseEntry 列表
│   └── preferences.json  # ToolPreference map
├── teachings/
│   ├── published.json    # 本 Role 发布的教学
│   └── subscribed/       # 订阅的其他 Role teachings (缓存)
└── sessions/
    └── 2026-06-11/
        └── session-1430.json  # 执行记录
```

### 运行时模型

```
┌─────────────────────────────────────────────────────────────┐
│                    RoleRuntime                               │
│                                                              │
│  ┌───────────────┐      ┌───────────────┐                    │
│  │  Scheduler    │      │  Sentinel     │                    │
│  │  cron 触发     │      │  budget 监控   │                    │
│  │  间隔执行      │      │  熔断保护      │                    │
│  └───────┬───────┘      └───────┬───────┘                    │
│          │                      │                            │
│          ▼                      ▼                            │
│  ┌───────────────────────────────────────┐                  │
│  │           Role::run_cycle()            │                  │
│  │  ┌───┐ → ┌───┐ → ┌───┐ → ... → ┌───┐ │                  │
│  │  │ R │   │ D │   │ P │         │ L │ │                  │
│  │  └───┘   └───┘   └───┘         └───┘ │                  │
│  └───────────────────────────────────────┘                  │
│          │                                                   │
│          ▼                                                   │
│  ┌──────────────────────────┐                               │
│  │       RoleStore          │   每轮结束 auto-save           │
│  │  读写 ~/.xuanji/roles/   │                               │
│  └──────────────────────────┘                               │
│          │                                                   │
│          └──────▶ TeachingLibrary (全局共享)                  │
└─────────────────────────────────────────────────────────────┘
```

### Sentinel 守护策略

| 事件 | 动作 |
|------|------|
| 目标队列挖空 | Role 进入 `idle`，等待下次 DISCOVER 发现新目标 |
| Per-role budget 耗尽 | 暂停循环，记录断点，等待用户 `xuanji budget reset` |
| 连续失败 N 次 | 自动降低优先级，将当前 goal 标记为 blocked |
| 新 teaching 到达（订阅的 Role 发布） | 注入下轮 REFLECT 的 review context |

### CLI 总览

```bash
# 生命周期
xuanji role hire <name> --purpose "..."       # 创建新 role
xuanji role fire <name>                       # 销毁 role
xuanji role list                              # 列出所有 roles
xuanji role show <name>                       # 查看详情

# 运行控制
xuanji role activate <name>                   # 启动自驱循环（前台）
xuanji role activate <name> --daemon          # 后台运行
xuanji role pause <name>                      # 暂停
xuanji role resume <name>                     # 恢复
xuanji role step <name>                       # 执行一轮（调试用）

# 进化
xuanji role evolve <name>                     # 手动触发深度反思
xuanji role teach <name>                      # 将当前知识打包为 teaching

# 知识
xuanji teaching list                          # 浏览教学库
xuanji teaching show <id>                     # 查看教学详情
xuanji teaching subscribe <role_name>         # 订阅

# 集成
xuanji daemon start                           # Role 在 daemon 中按 cron 运行
```

### 新增 Crate 概览

```
crates/
├── xuanji-role/          # 新增：Role 生命周期、自驱循环、学习引擎
│   ├── src/
│   │   ├── lib.rs        # Role struct + run_cycle
│   │   ├── types.rs      # RoleProfile, GoalNode, Rule, CaseEntry, Teaching
│   │   ├── discover.rs   # 目标发现 (DISCOVER + PRIORITIZE + DECOMPOSE)
│   │   ├── reflect.rs    # 回顾反思 (REFLECT + LEARN)
│   │   ├── store.rs      # RoleStore 持久化
│   │   └── teaching.rs   # TeachingLibrary 全局共享
│   └── Cargo.toml
├── xuanji-cli/           # 修改：新增 role 子命令
├── xuanji-agent/         # 不变
├── ...                   # 其余 8 个 crate 不变
└── xuanji-core/          # 可能微调：daemon 集成
```

### 设计原则回顾

| 原则 | 实现 |
|------|------|
| Role 自主权 | 目标自发现 + 按 seed_purpose 约束方向 |
| 多机制学习 | Rules(Condition→Action) + Cases(任务→方案) + Preferences(成功率信号) |
| 知识可复用 | TeachingLibrary 全局共享，跨 Role 传播验证 |
| 持续运行 | 自驱循环 + 间隔触发 + Sentinel 守护 |
| 持久化 | RoleStore 每轮结束 auto-save |
| 现有架构复用最大化 | Agent 执行任务，Memory/Bus/Budget 各司其职 |

---

## 五、God Role——系统级统筹者

### 定位

God Role 是整个 Role 生态的**元管理者**，不做具体业务执行，而是观察、理解和优化整个 Role 群体。

```
                    ┌─────────────┐
                    │   God Role   │  (默认创建, 始终存在)
                    │ "元管理者"    │
                    └──────┬──────┘
                           │ 观察 + 建议
          ┌────────────────┼────────────────┐
          ▼                ▼                 ▼
    ┌──────────┐    ┌──────────┐    ┌──────────┐
    │  Role-A   │    │  Role-B   │    │  Role-C   │
    │ 安全审计   │    │ 性能优化   │    │ 文档生成   │
    └──────────┘    └──────────┘    └──────────┘
```

### God Role 的特殊属性

| 属性 | 值 |
|------|---|
| seed_purpose | `"统筹管理所有 Role，发现协作机会，优化整体效率"` |
| 创建方式 | `xuanji init` 时自动 bootstrap，不可被 fire |
| 运行模式 | 始终活跃（daemon 启动时自动运行） |
| 工具权限 | 可调用 `role.*` 系统工具 |
| 知识权限 | 自动订阅所有 Teaching，可查询任何 Role 状态 |

### God Role 的能力

```
┌──────────────────────────────────────────────────────────┐
│              God Role 专属能力                             │
│                                                           │
│  1. 生态感知                                              │
│     ✓ 自动订阅所有 Role 的 Teaching                        │
│     ✓ 可查询各 Role 的进化阶段和目标队列                     │
│     ✓ 接收 bus 上所有 msg（不过滤源）                        │
│                                                           │
│  2. 协同发现                                              │
│     ✓ 发现 "Role-A 应该和 Role-B 交换知识"                  │
│     ✓ 发现 "此处缺少一个 Role 来负责 X"                     │
│     ✓ 推荐 "Role-B 当前的 goal 可以用 Role-A 的经验"        │
│                                                           │
│  3. 主动建议                                              │
│     例如：                                                 │
│     "Role-B 正重复 Role-A 已验证失败的路径，               │
│      建议其查阅 Teaching#003"                               │
│     "检测到项目新增 API 端点但无安全审计，                  │
│      建议 hire 一个 Security Role"                          │
│                                                           │
│  4. 被动响应                                              │
│     用户可直接和 God Role 对话：                             │
│     $ xuanji role chat god "项目哪些方面需要改进？"         │
│                                                           │
└──────────────────────────────────────────────────────────┘
```

### God Role 的系统工具

```rust
// God Role 通过系统工具获得对其他 Role 的管理能力

role.list                // 列出所有 Role + 状态摘要
role.inspect <name>      // 查看某 Role 的详情
role.hire <name>         // 建议创建新 Role（返回 Y/N + 理由）
role.suggest             // 主动分析：缺少哪个方向的 Role？
role.teachings           // 浏览教学库，发现可传播的知识
role.cross-link          // 分析两个 Role 是否存在协同机会
```

### 初始化流程

```bash
$ xuanji init
  ✅ 创建配置 ~/.xuanji/config.toml
  ✅ 初始化 God Role
      purpose: 统筹管理所有 Role
      status:  active (daemon 启动时运行)
  ✅ 初始化完成

$ xuanji role list
  NAME    STAGE     ACTIVE   GOALS
  god     Expert    yes      2          # 始终存在
```

### God Role 的自驱循环特点

与其他 Role 对比：

| | God Role | 普通 Role |
|---|---|---|
| DISCOVER 输入 | 整个 Role 生态 + Teaching 全局 | 自身 seed_purpose + 历史经验 |
| DISCOVER 产出 | 协同建议 / 缺失发现 / 知识路由 | 自身下一个目标 |
| 执行 | 发布 Teaching 路由 / 建议 hire | 直接操作工具执行任务 |
| 循环间隔 | 较长（如每小时），更宏观 | 较短（如每 5 分钟），更敏捷 |

---

## 六、God Role 作为默认入口

### 架构变更

```
当前:
  xuanji "prompt"   →  Agent.run()         (无上下文)
  xuanji chat       →  Agent.run() 循环     (无上下文)

目标:
  xuanji "prompt"   →  GodRole.run_goal()  (感知全局)
  xuanji chat       →  GodRole.chat()      (持续对话)
```

### God Role 交互模式

```
┌─────────────────────────────────────────────────────────┐
│                   xuanji CLI 入口                         │
│                                                          │
│  xuanji "列出所有角色"                                    │
│      │                                                   │
│      ▼                                                   │
│  ┌────────────────────────────────────┐                 │
│  │         God Role                    │                 │
│  │                                      │                 │
│  │  Context（每次对话自动加载）:          │                 │
│  │  ├─ 所有 Role 状态                  │                 │
│  │  ├─ 最近 Teaching 摘要              │                 │
│  │  ├─ 项目知识（LongTermMemory）       │                 │
│  │  └─ 对话历史（Chat 模式下累积）       │                 │
│  │                                      │                 │
│  │  prompt → run_goal(prompt) ──▶ 响应   │                 │
│  └────────────────────────────────────┘                 │
└─────────────────────────────────────────────────────────┘
```

### 与普通 Agent 的差异

| 维度 | 当前 Agent | God Role |
|------|-----------|----------|
| 视角 | 无历史，无全局 | 知道所有 Role、教学库、进化状态 |
| 记忆 | 单次对话 | 跨 session 持久化 |
| 工具 | shell.run + MCP | 同时拥有 `role.*` 系统工具 |
| 建议 | 无 | 可主动建议创建 Role、引入 Teaching |
| Chat | 简单循环 | 持续对话 + 每轮注入全局 context |
| 学习 | 无 | 对话中学到的自动归档为 Teaching |

### 关键改造点

**1. CLI 入口（`xuanji-cli/src/main.rs`）**

```rust
// 改造前:
(Some(prompt), None) => {
    commands::agent::run_agent(&prompt, ...).await?;
}

// 改造后:
(Some(prompt), None) => {
    commands::god::run_prompt(&prompt, &config).await?;
}

// 改造前:
(None, Some(Commands::Chat)) => {
    commands::agent::run_chat(...).await?;
}

// 改造后:
(None, Some(Commands::Chat)) => {
    commands::god::run_chat(&config).await?;
}
```

**2. God Role 初始化**

```rust
// xuanji init 时自动 bootstrap
pub fn bootstrap_god() -> Result<Role> {
    let god = Role::new(
        "god",
        "统筹管理所有 Role，发现协作机会，优化整体效率"
    )
        .with_stage(Stage::Expert)        // 初始即为 Expert
        .with_role_tools(/* role.* 系统工具 */)
        .activate()?;                     // 立即激活
}
```

**3. Chat 模式增强**

每轮对话自动注入：

```
## 当前全局状态
- 活跃 Role: 3 个 (security-auditor, doc-generator, perf-watcher)
- 最近 Teaching: 2 条 (Role-A 发布了 ...)
- 预算剩余: 80%
- 待处理建议: 1 条
```

### 完整 CLI 命令最终版

```bash
# 默认交互 —— God Role 处理
xuanji "列出所有角色的状态"              # 单次 Goal
xuanji chat                             # 连续对话

# Role 管理
xuanji role hire <name> --purpose "..." # 创建新 Role
xuanji role list                        # 列出所有 Role
xuanji role show <name>                 # 查看详情
xuanji role activate <name>             # 启动自驱
xuanji role chat <name>                 # 与指定 Role 对话
xuanji role fire <name>                 # 销毁
xuanji role evolve <name>               # 触发深度反思

# 教学库
xuanji teaching list                    # 浏览
xuanji teaching show <id>               # 详情
xuanji teaching subscribe <role>        # 订阅

# 工作流 (不变)
xuanji run <workflow.yaml>

# 守护进程 (不变)
xuanji daemon start / status / stop
```

### 目录结构总览

```
~/.xuanji/
├── config.toml
├── roles/
│   ├── god/                    ◀── 默认创建，永久存在
│   │   ├── profile.toml
│   │   ├── state.json
│   │   ├── queue/
│   │   │   ├── goals.json
│   │   │   └── archive/
│   │   ├── learning/
│   │   │   ├── rules.json
│   │   │   ├── cases.json
│   │   │   └── preferences.json
│   │   ├── teachings/
│   │   │   ├── published.json
│   │   │   └── subscribed/
│   │   └── sessions/
│   ├── security-auditor/       ◀── 用户 hire
│   └── doc-generator/          ◀── 用户 hire
└── teaching-library/           ◀── 全局共享
    ├── index.json
    └── entries/
```
