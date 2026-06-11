# Role 执行接线 Implementation Plan

> **For agentic workers:** This plan wires the existing God Role scaffold to the real execution layer. Execute in dependency order; compile after each milestone. Spec: `docs/superpowers/specs/2026-06-11-role-execution-wiring-design.md`.

**Goal:** Make God Role actually execute (real LLM + tools), inject per-role persona, unify memory into RoleStore, drive orchestration via LLM, and land auto-hire + tiered-safe-fire — ending with `cargo build` green, committed and pushed.

**Architecture:** Agent receives an injected persona string + rendered memory-context string and returns `ExecutionStats` (no held memory object → no `xuanji-role ↔ xuanji-agent` cycle). The CLI builds a shared `Arc<dyn LlmProvider>` + an `AgentFactory`; `xuanji-role` consumes both to build worker agents and make orchestration LLM calls. RoleStore becomes the single persistent memory (LongTermMemory retired on the role path).

**Tech Stack:** Rust 2024 workspace; tokio; serde; crates xuanji-{llm,plugin,memory,agent,role,core,cli}.

---

## File Structure

**Create:**
- `crates/xuanji-cli/src/commands/runtime.rs` — shared `create_provider` / `create_registry` / `render_markdown` (moved from agent.rs; dedup swarm.rs/workflow.rs copies) + `AgentFactory` impl.
- `crates/xuanji-role/src/orchestrate.rs` — LLM-driven `RoleOrchestrator` (decompose/match/aggregate) + `Assignment`.

**Modify:**
- `crates/xuanji-agent/src/types.rs` — add `ExecutionStats`.
- `crates/xuanji-agent/src/agent.rs` — `persona` + `memory_context` fields/builders; `run()` returns `ExecutionStats` with real counts; drop self-built LTM loading.
- `crates/xuanji-agent/src/prompt.rs` — `build_system_prompt(..., persona: Option<&str>)`.
- `crates/xuanji-llm/src/provider.rs` (or lib.rs) — `ArcProvider` newtype (shared provider).
- `crates/xuanji-role/src/types.rs` — `RoleContext`, `Assignment`.
- `crates/xuanji-role/src/store.rs` — `context` load/save, `archive`/`restore`, `list_archived`.
- `crates/xuanji-role/src/lib.rs` — `render_persona`, `render_context`, `AgentFactory` trait, `with_provider`/`with_agent_factory`, fix worker-purpose bug, real-stats reflect, wire `RoleOrchestrator`, auto-hire, tiered fire.
- `crates/xuanji-role/src/discover.rs` — keep as fallback (no change beyond comments).
- `crates/xuanji-cli/src/config.rs` — add `[role]` (`RoleCliConfig`).
- `crates/xuanji-cli/src/commands/{god,role,memory}.rs` — config plumbing, real agents, Expert stage, unified chat, archive fire, memory redirect.
- `crates/xuanji-cli/src/main.rs` — pass `&config` into god/role/memory dispatch.
- `crates/xuanji-cli/src/commands/mod.rs` — declare `runtime`.

---

## Chunk 1 — Foundation (types + provider sharing)

### Task 1.1: ExecutionStats + persona seam in agent crate
- Add `ExecutionStats { text, tool_calls, tokens, success }` to `xuanji-agent/src/types.rs`.
- `build_system_prompt` gains trailing `persona: Option<&str>`; if `Some`, prepend persona block before the hardcoded persona.
- `Agent`: add `persona: Option<String>`, `memory_context: Option<String>` fields + `with_persona`, `with_memory_context` builders.
- Change `Agent::run() -> Result<ExecutionStats>`: accumulate `tool_calls` (count each executed call) and `tokens` (sum `response.usage().total_tokens`); use `self.memory_context` instead of `load_memory_context()`; set `success = working.errors.is_empty()`. Keep `save_history` (no-op when LTM absent).
- Update in-crate call sites of `build_system_prompt` (agent.rs:210, 242) to pass `self.persona.as_deref()`.
- Test: unit test that `build_system_prompt` with persona Some contains the persona text.

### Task 1.2: ArcProvider in xuanji-llm
- Add `pub struct ArcProvider(pub Arc<dyn LlmProvider>)` implementing `LlmProvider` by delegation (reuse existing pattern in `xuanji-core/src/system_tools.rs` if pub; otherwise add to `xuanji-llm`).
- Enables one provider config → shared across God agent + N workers + orchestrator.

### Task 1.3: Role types + RoleStore context/archive
- `xuanji-role/src/types.rs`: add `RoleContext { notes: String, focus: String }`, `Assignment { task_idx, assignee: Option<String>, hire: Option<String> }`.
- `store.rs`: add `CONTEXT_FILE = "context.json"`, `save_context`/`load_context`; change `delete` → `archive` (move to `~/.xuanji/roles/.archived/<name>-<ts>/`), add `restore`, `list_archived`. Keep a thin `delete` that calls archive for compat (CLI `fire`).
- Tests: context roundtrip; archive then `list_roles` excludes / `.archived` contains; restore roundtrip.

### Task 1.4: RoleCliConfig
- `config.rs`: add `RoleCliConfig { auto_hire: bool (default true), fire_stale_days: i64 (default 7) }`; add `pub role: RoleCliConfig` to `XuanjiConfig` with `#[serde(default)]`.

## Chunk 2 — Role render + factory + bug fixes

### Task 2.1: render_persona / render_context
- `Role::render_persona()` → string from `self_description` + `seed_purpose` + `Stage`.
- `Role::render_context()` → load context + high-confidence rules + top cases + relevant teachings, render Markdown block.
- Tests for both.

### Task 2.2: AgentFactory + provider injection
- Define `pub trait AgentFactory: Send + Sync { async fn build(&self, role_name: &str, persona: &str, memory_context: &str) -> Agent; }` in `xuanji-role/src/lib.rs`.
- `Role` gains `agent_factory: Option<Arc<dyn AgentFactory>>` + setter; workers in orchestration built via factory (fresh provider via shared `ArcProvider`).
- Note: `xuanji-role` does NOT depend on provider/registry crates; factory is implemented in CLI.

### Task 2.3: Fix worker-purpose overwrite + real stats
- In `run_orchestrated_cycle`, before creating a worker, `load_profile(assignee)`; reuse existing purpose if present, else use the hire-suggested purpose.
- `reflect` consumes `ExecutionStats` → real `tool_calls_count`/`tokens_used`.
- Make `is_stale` threshold read from config (pass `fire_stale_days`).

## Chunk 3 — LLM orchestration + hire + fire

### Task 3.1: RoleOrchestrator (decompose/match/aggregate)
- `orchestrate.rs`: `RoleOrchestrator { provider: Arc<dyn LlmProvider> }`.
  - `decompose(goal) -> Vec<SubTask>` via one `complete` call, parse JSON list; fallback to `DiscoverEngine::decompose`.
  - `match(subtasks, roles) -> Vec<Assignment>` via `complete` with role roster; fallback to `Orchestrator::match_roles`.
  - `aggregate(results) -> String` via `complete`.
- Convert hire `Assignment`s → existing `OrchestrationSuggestion{HireRole}` for `CycleResult`.

### Task 3.2: Auto-hire
- When match yields `hire: Some(purpose)` and `auto_hire`: derive name (slug), `Role::new(name, purpose)` via factory, dispatch the subtask, record event. If `auto_hire=false`, only emit suggestion.

### Task 3.3: Tiered fire
- At cycle start: `Stage::Seed` + 0 cases + stale(>fire_stale_days) → `archive` automatically; roles with data → emit `FireRole` suggestion only. God never fired.

## Chunk 4 — CLI wiring + chat

### Task 4.1: runtime.rs + config plumbing
- Move `create_provider`/`create_registry`/`render_markdown` to `commands/runtime.rs`; repoint `swarm.rs`/`workflow.rs`/`agent.rs` to use them (remove dups).
- `CliAgentFactory` impl (holds `Arc<dyn LlmProvider>`, mcp_servers, agent_config, workflows_dir): `build()` → fresh `ArcProvider` clone + registry + register shell.run/workflow_create + `with_persona` + `with_memory_context`.
- `main.rs`: pass `&config` to `god::run_prompt`/`run_chat`/`role::handle_role`.

### Task 4.2: god.rs real wiring
- `bootstrap_god(&config)`: build shared provider, god's Agent (persona=render_persona, memory_context=render_context, Expert stage), `CliAgentFactory`, `RoleOrchestrator`; attach to Role.
- `run_orchestrated_cycle` now drives real orchestration; aggregate result rendered via `render_markdown` and printed.
- `run_chat(&config)`: god Agent in chat mode (long-lived) + each turn runs orchestrated cycle.

### Task 4.3: role.rs + memory.rs
- `role.rs handle_role(&action, &config)`: hire/fire(list/show use store; fire → archive; evolve/chat build real agent via factory).
- `memory.rs`: show/clear/rule operate on the active (god) role's RoleStore.

## Chunk 5 — Verify + ship

### Task 5.1: Compile green
- `cargo build --release` (and `cargo test` for role/agent crates). Resolve all ripple from `Agent::run` signature change (swarm.rs, system_tools.rs delegate, lib.rs).
### Task 5.2: Commit + push
- Stage all, commit with conventional message, push to `origin`.

---

## Notes for executor
- After Task 1.1, grep all `agent.run(` / `.run(prompt` call sites and update to `ExecutionStats` (`.text`).
- `xuanji-role` must NOT gain a dependency on `xuanji-llm`/`xuanji-plugin` for *construction*; it receives `Arc<dyn LlmProvider>` (xuanji-llm trait object) and `Arc<dyn AgentFactory>` only. Confirm `xuanji-role/Cargo.toml` adds `xuanji-llm` (trait + ArcProvider) but not plugin.
- Keep heuristic discover/match as fallback behind LLM-failure `Result`.
