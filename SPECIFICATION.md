# System Specification: AgenticEditor

## 1. Purpose and Scope

AgenticEditor is a **personal, local-first agentic AI editor** that owns the **entire software design life cycle** — from specification through delivery — using local inference (llama-cpp). It is not a code editor with an AI assistant bolted on; it is a **spec-driven, traceability-verified development pipeline** where the agent autonomously transforms a user's intent into working, tested software.

**Primary user:** A single developer (the owner).

**Success criteria:** Given a goal, the agent completes multi-step coding tasks autonomously with minimal user intervention. The user may be absent for long stretches; the agent must never block on the user.

**Differentiators (features other editors lack):**

- Full SDLC ownership: spec → WBS → TDD implementation → delivery, as one verifiable pipeline.
- Requirements traceability: every requirement traces through WBS items to passing tests.
- Spec-driven flow with structured acceptance criteria as the test oracle.
- Autonomous operation with an assumption ledger and deferral queue for review on return.
- Context-isolated subagents (Explore) for focused capabilities without polluting the worker's context.
- Declarative agent system (YAML graph + instruction files) — the agent topology is data, not code.

---

## 2. Domain Model

### 2.1 Entities

| Entity               | Description                                     | Key Attributes                                                                                                              |
| -------------------- | ----------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------- |
| **Requirement**      | A testable behavior from the spec               | `id`, `text`, `acceptance_criteria` (given/when/then), `state` (planned / not-planned / covered)                            |
| **WBS Item**         | A unit of work derived from the spec            | `id`, `title`, `requirement_ids` (FK), `depends_on` (FK, self-ref), `state` (planned / in-progress / covered / not-planned) |
| **Assumption**       | A decision the agent made unilaterally          | `id`, `item_id` (FK), `description`, `rationale`, `state` (assumed / confirmed / rejected)                                  |
| **Deferral**         | An item marked not-planned due to infeasibility | `id`, `item_id` (FK), `reason`, `conflicting_requirement_ids`, `state` (deferred / resolved / dropped)                      |
| **Agent**            | A role or capability in the agent system        | `name`, `type` (role / capability), `instructions_file`, `tools[]`, `model`, `invoked_by[]`                                 |
| **Skill**            | A reusable procedure (SKILL.md)                 | `name`, `path`, `description`                                                                                               |
| **Memory**           | Tiered persistent knowledge                     | `scope` (session / repo / user), `key`, `content`                                                                           |
| **Architecture Doc** | The canonical ideal-architecture document       | `path`, `content` (markdown)                                                                                                |

### 2.2 Relationships

- A **Requirement** is covered by one or more **WBS Items** (many-to-many via `requirement_ids`).
- A **WBS Item** depends on zero or more other **WBS Items** (DAG, acyclic).
- A **WBS Item** may produce zero or more **Assumptions**.
- A **WBS Item** may be deferred, producing one **Deferral**.
- An **Agent** (role) may invoke zero or more **Agents** (capabilities or roles) as subagents.
- An **Agent** may load zero or more **Skills** when relevant to the task.
- **Memory** is scoped: session (ephemeral), repo (per-project), user (cross-project).

### 2.3 State Transitions

**Requirement states:**

```
planned ──→ covered          (all mapped items are covered)
planned ──→ not-planned      (all mapped items are deferred)
```

**WBS Item states:**

```
planned ──→ in-progress ──→ covered       (tests pass, committed)
planned ──→ in-progress ──→ not-planned   (infeasible, deferred)
planned ──→ not-planned                   (skipped due to dependency deferral)
```

**Assumption states:**

```
assumed ──→ confirmed        (user approves on review)
assumed ──→ rejected         (user rejects; triggers rework)
```

**Deferral states:**

```
deferred ──→ resolved        (user provides resolution; item re-enters as planned)
deferred ──→ dropped         (user confirms it should not be done)
```

---

## 3. Functional Requirements

### 3.1 The Pipeline (Four Stages)

The core workflow is a four-stage pipeline with verification gates:

```
┌──────────┐     ┌──────────┐     ┌──────────────┐     ┌──────────┐
│  Stage 1 │     │  Stage 2 │     │   Stage 3    │     │  Stage 4 │
│   SPEC   │────▶│   WBS    │────▶│  TDD PER     │────▶│ DELIVER  │
│          │     │          │     │  ITEM        │     │          │
└──────────┘     └──────────┘     └──────────────┘     └──────────┘
     │                │                  │                   │
  Gate:            Gate:            Gate:               Gate:
  User           Coverage +       All tests           Suite green +
  confirms       acyclicity       pass + Analyzer     traceability
                                   score handled       complete
```

#### Stage 1: Specification

- **Participants:** User + Worker (interactive mode).
- **Process:**
  1. User states a goal (e.g., "make a calculator").
  2. Worker asks Socratic questions to resolve ambiguity.
  3. Worker drafts a structured spec: requirements with machine-checkable acceptance criteria (given/when/then).
  4. User reviews, corrects, and confirms.
- **Output:** Structured requirements in the SQLite store.
- **Gate:** User explicitly confirms the spec is complete.
- **Constraint:** This stage REQUIRES the user. If the user is absent, the agent waits.

#### Stage 2: Work Breakdown Structure

- **Participants:** Worker (autonomous).
- **Process:**
  1. Worker reads all requirements from the store.
  2. Worker breaks them into WBS items (phases → tasks → subtasks), each mapped to the requirements it covers.
  3. Worker builds the dependency DAG (what must exist before what).
  4. Worker runs verification:
     - **Coverage check:** every requirement maps to ≥1 item.
     - **Acyclicity check:** topological sort succeeds (no cycles).
  5. Worker writes items to the SQLite store.
- **Output:** WBS items in the SQLite store with requirement mappings and dependencies.
- **Gate:** Coverage + acyclicity pass.
- **On failure:**
  - Coverage gap due to spec silence → Worker assumes a reasonable default, records an **Assumption**, continues.
  - Cycle detected → Worker restructures. If unresolvable → marks affected items `not-planned`, records a **Deferral**, continues with the rest.

#### Stage 3: TDD Implementation (per item)

- **Participants:** Worker (autonomous), Analyzer (subagent, after each item).
- **Process (for each item in topological order):**
  1. Worker reads the item's requirements + acceptance criteria.
  2. Worker writes **tests first**, derived mechanically from the acceptance criteria.
  3. Worker implements until the new tests pass.
  4. Worker runs the **full test suite** (regression check).
  5. Worker commits (git).
  6. Worker launches **Analyzer** (subagent):
     - Holistic architecture review.
     - Selects exactly one scored improvement (1–10).
     - Updates the architecture doc if triggers apply.
     - Score > 5 → Worker implements the fix, commits.
     - Score ≤ 5 → recorded, moves on.
- **Output:** Working code + passing tests + updated architecture doc + git commit.
- **Gate:** All tests pass (new + existing). Analyzer score handled.
- **On failure:**
  - Tests won't pass after bounded retries:
    - **Spec gap** (spec is silent) → Worker assumes, records an **Assumption**, implements, flags for review.
    - **Infeasibility** (spec is contradictory) → Worker marks item `not-planned`, records a **Deferral** with the exact conflict, **continues with other items**.
  - Full suite regression → Worker fixes before proceeding. If unfixable → reverts the item's commit, marks `not-planned`.

#### Stage 4: Delivery

- **Participants:** Worker (prepares), User (reviews).
- **Process:**
  1. Worker runs the full test suite one final time.
  2. Worker builds the **traceability matrix**: every requirement → items → tests → pass/fail.
  3. Worker compiles the **assumption ledger**: all assumptions with rationale.
  4. Worker compiles the **deferral queue**: all deferrals with reasons.
  5. Worker presents: what was built, what was assumed, what was deferred.
- **Output:** Working software + traceability matrix + assumption ledger + deferral queue.
- **Gate:** Full suite green + traceability complete (no `planned` requirement left untested).
- **On user review:**
  - Rejected assumptions → Worker reworks affected items.
  - Resolved deferrals → Worker re-enters items as `planned`, implements.

### 3.2 Agent System

#### 3.2.1 Architecture

The agent system is **declarative**: a YAML graph file defines the topology; per-agent instruction files define behavior.

- **`agents.yaml`** (the graph):
  - Defines all agents: name, type (role/capability), tools, model, instructions file path.
  - Defines invocation edges: which agents can invoke which as subagents.
- **Per-agent instruction files** (`.md`):
  - Each agent's standing instructions, constraints, and procedures.
  - Referenced by the graph.

#### 3.2.2 Roles (pipeline participants)

| Role         | Stages              | Responsibility                                                                                                             |
| ------------ | ------------------- | -------------------------------------------------------------------------------------------------------------------------- |
| **Worker**   | 1, 2, 3, 4          | Interactive spec authoring, WBS generation, TDD implementation, delivery preparation. The primary agent the user talks to. |
| **Analyzer** | 3 (after each item) | Holistic architecture review, one scored improvement, maintains the architecture doc. Invoked by Worker as a subagent.     |

#### 3.2.3 Capabilities (context-isolated subagents)

| Capability  | Invoked by       | Purpose                                                                                                                                                                       |
| ----------- | ---------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Explore** | Worker, Analyzer | Fast read-only codebase exploration + web research. Context-isolated: does NOT know the invoker's prior actions. Searches fresh, reports concisely. Safe to call in parallel. |

**Design principle:** Subagents are useful not only for independent business roles but for **context isolation**. A capability agent's ignorance of the invoker's full context is a feature — it keeps the capability focused and prevents context pollution.

#### 3.2.4 Skills

- Skills are `SKILL.md` files, auto-discovered from a known directory.
- Agents load skills when relevant to the current task.
- Skills are reusable procedures (e.g., `subagent-analysis`, `create-skill`).
- Skills are separate from the agent graph — they are loaded _by_ agents, not _part of_ the graph.

### 3.3 Memory System

Tiered persistent knowledge:

| Scope       | Lifetime                  | Contents                                                                                      |
| ----------- | ------------------------- | --------------------------------------------------------------------------------------------- |
| **Session** | Current task/session      | Working state: current goal, in-progress plan, recent context. Cleared when the session ends. |
| **Repo**    | Per-project, persistent   | Architecture facts, conventions, verified commands, decisions/ADRs. Lives with the project.   |
| **User**    | Cross-project, persistent | Personal preferences, common workflows, patterns. Survives across all projects.               |

### 3.4 Project Management (Spec-Driven Flow)

- The pipeline IS the project management system.
- Transitions between stages are **agent-driven with user approval**:
  - Agent proposes the next stage (e.g., "spec is ready, shall I generate the WBS?").
  - User approves before the agent proceeds.
  - Exception: within Stage 3 (item-to-item), the agent proceeds autonomously.
- WBS granularity: **fine** — phases → tasks → subtasks.
- The project board reflects the live state of all items.

### 3.5 User Actions

| Action                         | Description                                                                        |
| ------------------------------ | ---------------------------------------------------------------------------------- |
| **Start a spec**               | User describes a goal; Worker enters interactive spec mode.                        |
| **Confirm spec**               | User approves the spec; triggers WBS generation.                                   |
| **Approve WBS**                | User reviews the WBS; approves or requests changes.                                |
| **Monitor progress**           | User watches the live project board as items transition (real-time via WebSocket). |
| **Review delivery**            | User reviews the traceability matrix, assumption ledger, and deferral queue.       |
| **Confirm/reject assumptions** | User reviews assumptions; rejects trigger rework.                                  |
| **Resolve/drop deferrals**     | User decides on deferred items.                                                    |
| **Open file / edit code**      | Traditional editor interaction (file browser, code editor).                        |
| **Run command**                | User can run terminal commands directly.                                           |
| **Search codebase**            | User can invoke Explore directly or let the agent do it.                           |
| **View streaming output**      | User watches agent reasoning, tool calls, and terminal output stream in real-time. |
| **View file changes**          | User sees live diffs when the agent edits files.                                   |

### 3.6 Real-Time Features (WebSocket)

The server pushes events to all connected clients via WebSocket. Four event streams:

| Stream            | Content                                                                | Frequency                 |
| ----------------- | ---------------------------------------------------------------------- | ------------------------- |
| **Agent output**  | Streaming tokens, tool calls, reasoning steps                          | Per-token / per-tool-call |
| **Project board** | Item state transitions (planned → in-progress → covered / not-planned) | Per state change          |
| **Terminal**      | Command output, test results, build output                             | Per output chunk          |
| **File changes**  | File created/modified/deleted, with diff                               | Per file operation        |

All four streams work identically on any connected device (PC browser, phone browser).

### 3.7 UI Surfaces

The SolidJS SPA provides six main surfaces, accessible from any device on the LAN:

| Surface                        | Description                                                                                                                         |
| ------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------- |
| **Code editor + file browser** | A code editor pane with a file tree. Syntax highlighting, file open/save. The traditional editing experience.                       |
| **Agent chat**                 | A conversation panel where the user directs the agent and sees its streaming reasoning, tool calls, and responses.                  |
| **Project board**              | A kanban/list view of WBS items, grouped by state (planned, in-progress, covered, not-planned). Updates in real-time via WebSocket. |
| **Spec / document viewer**     | A pane to read and edit the structured spec, WBS, and architecture doc.                                                             |
| **Diff / review view**         | A view to inspect agent-proposed file changes with unified diff. Used during delivery review.                                       |
| **Terminal / output**          | A terminal pane showing command output, test results, and build output. Streams in real-time.                                       |

**Responsive design:** The UI must be usable on both a desktop browser (PC) and a mobile browser (phone). On mobile, surfaces may be tabbed or stacked; on desktop, they may be arranged in a multi-pane layout.

### 3.8 Business Rules

1. **Acceptance criteria are the oracle.** Tests in Stage 3 are derived from the spec's acceptance criteria, NOT from the implementation. This prevents test–implementation collusion.
2. **Full suite always runs.** After every item, the entire test suite must pass — not just the new tests.
3. **Commit after every item.** Git is the rollback mechanism. Each item = one or more commits.
4. **Analyzer runs after every item.** No exceptions. Score > 5 is a mandatory fix.
5. **Assumptions are always recorded.** The agent never silently decides — every unilateral decision is logged with rationale.
6. **Deferrals always continue.** An infeasible item never blocks the pipeline. The agent marks it and moves on.
7. **The architecture doc sits ahead of the code.** It describes the ideal, not the current state. It is updated by the Analyzer, not the Worker.
8. **The agent never blocks on the user.** If a decision is needed and the user is absent, the agent assumes (if reversible) or defers (if destructive).

### 3.9 Workflows

#### Primary Workflow: Goal → Software

```
User: "Build X"
  → Worker (interactive): Socratic spec authoring
  → User confirms spec
  → Worker (autonomous): WBS generation + verification
  → User approves WBS (or is absent → agent proceeds)
  → Worker (autonomous): TDD loop per item
      → For each item:
          → Write tests from acceptance criteria
          → Implement
          → Run full suite
          → Commit
          → Launch Analyzer → handle score
  → Worker: Delivery preparation
  → User reviews: traceability + assumptions + deferrals
  → (Optional) Rework rejected assumptions / resolve deferrals
```

#### Failure Workflow: Infeasibility

```
Worker hits infeasibility on item T5
  → Detects conflict between R2 and R7
  → Marks T5 as not-planned
  → Records Deferral: "T5 infeasible: R2 (use floats) conflicts with R7 (exact representation)"
  → Continues with T6, T7, ...
  → At delivery: T5 appears in the deferral queue
  → User resolves: "Drop R7" or "Change R2 to arbitrary precision"
  → Worker re-enters T5 as planned, implements
```

#### Failure Workflow: Spec Gap

```
Worker hits spec silence on item T3 (e.g., float precision for 1/3)
  → Assumes: "Display 6 decimal places, round half-up"
  → Records Assumption: "T3: float display precision = 6 decimal places (not specified in spec)"
  → Implements with the assumption
  → At delivery: assumption appears in the ledger
  → User confirms or rejects
```

---

## 4. Edge Cases and Error Handling

| Scenario                                                      | Behavior                                                                                                                   |
| ------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------- |
| Spec is silent on a detail (e.g., output format)              | Agent assumes a reasonable default, records an Assumption, continues. Flagged for review.                                  |
| Spec is internally contradictory (e.g., R2 conflicts with R7) | Agent detects, marks affected items `not-planned`, records a Deferral with the exact conflict, continues with other items. |
| Tests won't pass after N retries (N = bounded, e.g., 3)       | Agent diagnoses: spec gap → assume; infeasibility → defer; bug → fix.                                                      |
| Full suite regression after an item                           | Agent fixes the regression. If unfixable → reverts the item's commit, marks `not-planned`.                                 |
| WBS has a cycle                                               | Agent restructures. If unresolvable → marks cyclic items `not-planned`, continues with the rest.                           |
| WBS has a coverage gap (requirement with no items)            | Agent creates the missing item. If it can't (spec too vague) → assumes + records.                                          |
| Analyzer scores > 5 but the fix is large                      | Agent implements the fix (mandatory). If the fix would break other items → defers the fix, records it.                     |
| User rejects an assumption on review                          | Agent reworks the affected item(s) with the corrected requirement.                                                         |
| User resolves a deferral                                      | Agent re-enters the item as `planned`, implements it.                                                                      |
| Machine restarts mid-run                                      | Agent resumes from the last git commit. Re-derives state from the SQLite store.                                            |
| llama-cpp server unavailable                                  | Agent reports the error, waits for the server to come back. Does not proceed without inference.                            |
| Disk space / resource exhaustion                              | Agent reports the error, pauses. Does not delete data to make space.                                                       |
| User is absent for a long time                                | Agent proceeds autonomously through all stages. Accumulates assumptions and deferrals. Presents everything at delivery.    |
| Two items are independent (no dependency)                     | Agent may process them in any order (or in parallel if the architecture supports it).                                      |
| An item's dependency is deferred                              | The dependent item is also marked `not-planned` (cascading deferral), with a note explaining why.                          |

---

## 5. Non-Functional Requirements

- **Responsiveness:** The UI must remain responsive during agent runs. All real-time features (streaming agent output, live project board, terminal output, file change notifications) use WebSocket push — no polling. The server must handle concurrent WebSocket connections (PC + phone simultaneously).
- **Local inference speed:** llama-cpp inference must be fast enough for interactive use. Target: ≥ 20 tokens/sec for the default model on the user's hardware. (Exact target TBD based on hardware.)
- **Data safety:** The SQLite store lives in a project-adjacent directory (e.g., `~/.agentic-editor/<project-hash>/store.db`), NOT inside the repo. Git history is the rollback mechanism for code.
- **Extensibility:** Adding a new agent, tool, or model provider should be a configuration change (edit `agents.yaml` or add a skill file), not a code change.
- **Polyglot:** The editor must work across any programming language and stack. Language-specific tooling (formatters, linters, test runners) is discovered per-project.
- **Network access:** The server runs on the PC and is accessible from any device on the local network (e.g., a phone browser). All real-time features work identically regardless of the client device.
- **Security:** MAC address allowlist. The server only accepts WebSocket/HTTP connections from devices whose MAC address is in a configured allowlist. All other connections are rejected at the connection level. The allowlist is stored in the server config (not in the repo).

---

## 6. Data Requirements

### 6.1 Storage: SQLite

- **Location:** Project-adjacent directory (e.g., `~/.agentic-editor/<project-hash>/store.db`), NOT inside the repo.
- **Schema:**

```sql
-- Requirements (from the spec)
CREATE TABLE requirements (
    id              TEXT PRIMARY KEY,       -- e.g., "R1"
    text            TEXT NOT NULL,          -- the requirement statement
    acceptance_criteria TEXT NOT NULL,      -- given/when/then, structured
    state           TEXT NOT NULL DEFAULT 'planned'
                    CHECK (state IN ('planned', 'not-planned', 'covered'))
);

-- WBS Items
CREATE TABLE items (
    id              TEXT PRIMARY KEY,       -- e.g., "T1"
    title           TEXT NOT NULL,
    state           TEXT NOT NULL DEFAULT 'planned'
                    CHECK (state IN ('planned', 'in-progress', 'covered', 'not-planned')),
    created_at      TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Item → Requirement mapping (many-to-many)
CREATE TABLE item_requirements (
    item_id         TEXT NOT NULL REFERENCES items(id),
    requirement_id  TEXT NOT NULL REFERENCES requirements(id),
    PRIMARY KEY (item_id, requirement_id)
);

-- Item dependencies (DAG edges)
CREATE TABLE item_dependencies (
    item_id         TEXT NOT NULL REFERENCES items(id),
    depends_on      TEXT NOT NULL REFERENCES items(id),
    PRIMARY KEY (item_id, depends_on)
);

-- Assumptions (agent decisions)
CREATE TABLE assumptions (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    item_id         TEXT NOT NULL REFERENCES items(id),
    description     TEXT NOT NULL,
    rationale       TEXT NOT NULL,
    state           TEXT NOT NULL DEFAULT 'assumed'
                    CHECK (state IN ('assumed', 'confirmed', 'rejected')),
    created_at      TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Deferrals (infeasible items)
CREATE TABLE deferrals (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    item_id         TEXT NOT NULL REFERENCES items(id),
    reason          TEXT NOT NULL,
    conflicting_requirement_ids TEXT,       -- comma-separated or JSON
    state           TEXT NOT NULL DEFAULT 'deferred'
                    CHECK (state IN ('deferred', 'resolved', 'dropped')),
    created_at      TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Memory (tiered)
CREATE TABLE memory (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    scope           TEXT NOT NULL CHECK (scope IN ('session', 'repo', 'user')),
    key             TEXT NOT NULL,
    content         TEXT NOT NULL,
    updated_at      TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE (scope, key)
);
```

### 6.2 Files in the Repo

- **Code:** The actual implementation (any language).
- **Tests:** Derived from acceptance criteria.
- **Architecture doc:** The canonical ideal-architecture document (markdown, maintained by Analyzer).
- **`agents.yaml`:** The agent graph (if project-specific overrides are needed).
- **Skills:** `SKILL.md` files (if project-specific).

### 6.3 Files Outside the Repo

- **SQLite store:** The structured spec, WBS, assumptions, deferrals, memory.
- **Agent instruction files:** Per-agent `.md` files (global or project-specific).
- **llama-cpp models:** Downloaded model files.

---

## 7. External Dependencies

| Dependency               | Purpose                             | Notes                                                                                    |
| ------------------------ | ----------------------------------- | ---------------------------------------------------------------------------------------- |
| **llama-cpp**            | Local LLM inference                 | Primary (only) model provider for now. Architecture should allow adding providers later. |
| **Git**                  | Version control, rollback           | Each item = commit(s). Rollback = git revert/reset.                                      |
| **Project toolchain**    | Build, test, lint, format           | Discovered per-project (package.json, Cargo.toml, etc.). Polyglot.                       |
| **Web (via Explore)**    | Documentation research, API lookups | Context-isolated; results reported concisely to the invoker.                             |
| **Node.js / TypeScript** | Server runtime                      | Backend server: WebSocket, file I/O, process management, SQLite.                         |
| **SolidJS**              | Frontend framework                  | SPA: code editor, file browser, chat, project board, terminal, diff view.                |
| **WebSocket**            | Real-time communication             | Four event streams: agent output, project board, terminal, file changes.                 |

---

## 8. Constraints and Assumptions

### Technical Constraints

- **Node/TypeScript backend + SolidJS frontend.** The server is a Node.js/TypeScript application. The frontend is a SolidJS single-page app.
- **WebSocket for real-time:** All streaming features (agent output, project board updates, terminal output, file change notifications) use WebSocket. The server pushes events to connected clients.
- **llama-cpp only** for model inference (for now). Provider abstraction should allow adding cloud APIs later.
- **MAC address allowlist:** The server validates the source MAC address of every incoming connection against a configured allowlist. Non-allowlisted connections are rejected.
- **Single-user:** No authentication beyond the MAC allowlist. No multi-user support.

### Business Constraints

- **Single user.** No multi-user, no authentication beyond MAC allowlist, no permissions model.
- **Local-first.** No cloud dependency for core functionality. Web access is for research only (via Explore).
- **LAN-accessible.** The server runs on the PC and is accessible from other devices on the local network (e.g., a phone). The user may interact with the editor from any device on the LAN.
- **User may be absent.** The system must handle long autonomous runs without blocking.
- **Machine assumed to stay on** for the duration of a run (resumability via git commit, not full checkpointing).

### Assumptions

- The user's hardware can run llama-cpp at acceptable speed for the chosen model.
- The project uses git (or will be initialized with git).
- The user is comfortable reviewing a batch of assumptions/deferrals at delivery time.

---

## 9. Acceptance Criteria

The specification is correctly implemented when:

1. **Spec authoring:** A user can describe a goal in natural language and the Worker produces a structured spec with testable acceptance criteria. The user can confirm or revise it.
2. **WBS generation:** From a confirmed spec, the Worker produces a WBS with full coverage (every requirement mapped) and an acyclic dependency graph.
3. **TDD implementation:** For each WBS item, tests are written from acceptance criteria BEFORE implementation. The full suite passes after each item.
4. **Traceability:** At delivery, every `planned` requirement has at least one passing test. The traceability matrix is complete.
5. **Autonomy:** The agent can run the full pipeline (Stages 2–4) without user interaction. It records assumptions and deferrals rather than blocking.
6. **Review:** On return, the user sees: the live project board (all items and their states), the assumption ledger, and the deferral queue.
7. **Rollback:** Any item can be rolled back via git. The SQLite store reflects the rolled-back state.
8. **Analyzer:** After every item, the Analyzer runs, produces exactly one scored improvement, and updates the architecture doc when triggers apply.
9. **Explore:** The Worker can invoke Explore for codebase search or web research. Explore operates in isolation and reports concisely.
10. **Extensibility:** A new agent can be added by editing `agents.yaml` and creating an instruction file — no code changes.

---

## 10. Open Questions

_(Empty — all questions resolved.)_
