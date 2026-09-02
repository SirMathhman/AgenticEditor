# MVP Specification: AgenticEditor Pipeline

## 1. Purpose

Prove that a single agent can autonomously transform a user's goal into working, tested code through a **spec → WBS → TDD → delivery** pipeline with requirements traceability.

**This is a CLI tool.** No web UI, no WebSocket, no subagents. One agent, one terminal, one project.

**Success criteria:** Given a goal (e.g., "make a calculator"), the agent produces a working, tested program where every requirement in the spec traces to at least one passing test.

---

## 2. Scope

### In

| Component              | Description                                                                                                 |
| ---------------------- | ----------------------------------------------------------------------------------------------------------- |
| **Stage 1: Spec**      | Interactive CLI conversation → structured spec (requirements + acceptance criteria) in SQLite               |
| **Stage 2: WBS**       | Agent breaks spec into work items with requirement mappings and dependencies. Coverage + acyclicity checks. |
| **Stage 3: TDD**       | For each item (topological order): write tests from AC → implement → run full suite → git commit            |
| **Stage 4: Delivery**  | Print traceability matrix: every requirement → items → tests → pass/fail                                    |
| **llama-cpp**          | Local inference via llama-cpp server (HTTP API)                                                             |
| **SQLite**             | Structured store: requirements, items, dependencies, item-requirement mappings                              |
| **Git**                | Version control. Commit after every item.                                                                   |
| **File I/O**           | Read/write/edit files in the project directory                                                              |
| **Terminal execution** | Run build/test/lint commands, capture output                                                                |

### Out (deferred to post-MVP)

| Component                          | Why deferred                                            |
| ---------------------------------- | ------------------------------------------------------- |
| Analyzer / architecture review     | Not needed to prove the pipeline works                  |
| Explore / subagents                | Single agent is sufficient for MVP                      |
| Assumption ledger / deferral queue | Agent logs decisions to console; no structured tracking |
| Tiered memory                      | Each run starts fresh                                   |
| Skills system                      | Agent instructions are hardcoded                        |
| Declarative agent system (YAML)    | One hardcoded agent                                     |
| Architecture doc maintenance       | No Analyzer                                             |
| Web UI / WebSocket / SolidJS       | CLI only                                                |
| MAC allowlist / LAN access         | Localhost only                                          |
| Cascading deferrals                | If an item is infeasible, agent skips it and notes it   |
| Rework on rejected assumptions     | No structured review loop                               |

---

## 3. Architecture

```
┌─────────────────────────────────────────────────┐
│                  CLI (Node/TS)                  │
│                                                 │
│  ┌───────────┐  ┌──────────┐  ┌─────────────┐  │
│  │  Stage 1  │  │ Stage 2  │  │  Stage 3    │  │
│  │  (Spec)   │→ │  (WBS)   │→ │  (TDD loop) │  │
│  └───────────┘  └──────────┘  └──────┬──────┘  │
│                                      │         │
│  ┌───────────┐                       │         │
│  │ Stage 4   │←──────────────────────┘         │
│  │(Delivery) │                                 │
│  └───────────┘                                 │
│                                                 │
│  ┌──────────────────────────────────────────┐   │
│  │           Agent (single, hardcoded)      │   │
│  │  - llama-cpp (HTTP)                      │   │
│  │  - File I/O                              │   │
│  │  - Terminal execution                    │   │
│  │  - Git                                   │   │
│  └──────────────────────────────────────────┘   │
│                                                 │
│  ┌──────────────────────────────────────────┐   │
│  │           SQLite (project-adjacent)      │   │
│  │  requirements, items, dependencies,      │   │
│  │  item_requirements                        │   │
│  └──────────────────────────────────────────┘   │
└─────────────────────────────────────────────────┘
```

**Stack:**

- **Runtime:** Node.js 20+, TypeScript
- **SQLite:** `better-sqlite3` (synchronous, simple)
- **llama-cpp:** HTTP client to a local llama-cpp server (e.g., `llama-cpp-server` on `localhost:8080`)
- **Git:** `simple-git` (or `child_process` with `git` CLI)
- **CLI:** `commander` or `inquirer` for interactive prompts
- **Project directory:** The user specifies a target project directory. The agent reads/writes files there.

**Storage location:** SQLite DB at `<project-dir>/.agentic/store.db` (inside the project but in a hidden dir; the agent's file operations are scoped to the project dir).

---

## 4. Functional Requirements

### 4.1 Stage 1: Specification

**Trigger:** User runs `agentic spec "make a calculator"` (or similar).

**Process:**

1. Agent enters interactive mode.
2. Agent asks Socratic questions (one at a time, or batched) to resolve ambiguity.
   - Questions are printed to the console.
   - User types answers.
   - Agent continues until it has enough to draft a spec.
3. Agent drafts a structured spec:
   - Each requirement has: `id` (R1, R2, ...), `text`, `acceptance_criteria` (given/when/then format).
4. Agent prints the draft spec to the console.
5. User reviews:
   - `y` → confirm (proceed to Stage 2)
   - `edit` → user provides corrections, agent revises
   - `n` → start over
6. On confirmation, agent writes requirements to SQLite.

**Output:** Rows in the `requirements` table.

**Gate:** User explicitly confirms.

**Constraints:**

- The agent MUST produce acceptance criteria in a testable format (given/when/then or equivalent).
- If the user's goal is too vague, the agent MUST ask clarifying questions before drafting.
- The agent MUST NOT assume requirements the user did not state.

### 4.2 Stage 2: Work Breakdown Structure

**Trigger:** Automatic after Stage 1 confirmation (or `agentic wbs` to re-run).

**Process:**

1. Agent reads all requirements from SQLite.
2. Agent breaks them into WBS items:
   - Each item: `id` (T1, T2, ...), `title`, `requirement_ids` (which requirements it covers), `depends_on` (which items must complete first).
   - Granularity: fine (phases → tasks → subtasks).
3. Agent runs verification:
   - **Coverage:** every requirement has ≥1 item. If not, agent creates the missing item.
   - **Acyclicity:** topological sort succeeds. If a cycle exists, agent restructures.
4. Agent writes items + mappings + dependencies to SQLite.
5. Agent prints the WBS to the console (item list with dependencies and requirement mappings).
6. User reviews:
   - `y` → confirm (proceed to Stage 3)
   - `edit` → user provides corrections
   - `n` → regenerate

**Output:** Rows in `items`, `item_requirements`, `item_dependencies` tables.

**Gate:** Coverage + acyclicity pass. User confirms.

**On failure:**

- Coverage gap → agent creates the missing item (may need to ask the user for clarification).
- Cycle → agent restructures. If unresolvable → agent reports the cycle and asks the user how to proceed.

### 4.3 Stage 3: TDD Implementation

**Trigger:** Automatic after Stage 2 confirmation (or `agentic implement` to run/resume).

**Process (for each item in topological order):**

1. Agent prints: `→ T3: Evaluator (covers R1, R3)`
2. Agent reads the item's requirements + acceptance criteria from SQLite.
3. Agent writes **tests first**, derived from the acceptance criteria.
   - Tests are written to the project's test directory.
   - Test names reference the requirement (e.g., `test_R1_addition`).
4. Agent implements the item.
   - Files are created/modified in the project directory.
5. Agent runs the **full test suite** (not just new tests).
   - If tests fail → agent fixes and re-runs (bounded retries: 3).
   - If still failing after 3 retries → agent prints the failure, marks the item as `not-planned`, and **continues to the next item**.
6. Agent commits to git: `git add -A && git commit -m "T3: Evaluator (R1, R3)"`.
7. Agent prints: `✓ T3: Evaluator — 5 tests passing`

**Output:** Working code + passing tests + git commit per item.

**Gate:** All tests pass (new + existing).

**On failure (after 3 retries):**

- Agent prints: `✗ T3: Evaluator — tests failing after 3 attempts. Skipping.`
- Agent marks the item `not-planned` in SQLite.
- Agent continues to the next item in topological order.
- If a dependent item is skipped, the agent also skips it (prints: `⊘ T4: REPL — skipped (depends on T3)`).

**Constraints:**

- Tests MUST be derived from the spec's acceptance criteria, NOT from the implementation.
- The full test suite MUST run after every item (regression check).
- The agent MUST NOT modify existing tests to make new ones pass.
- The agent MUST NOT delete existing tests.

### 4.4 Stage 4: Delivery

**Trigger:** Automatic after all items are processed (or `agentic deliver`).

**Process:**

1. Agent runs the full test suite one final time.
2. Agent builds the traceability matrix from SQLite:
   - For each requirement: which items cover it, which tests exist, pass/fail status.
3. Agent prints the matrix to the console:

```
DELIVERY REPORT
===============

Requirements:
  R1 (supports +,-,*,/)     → T1, T3  → 12 tests  ✓ PASS
  R2 (precedence)           → T2      →  4 tests  ✓ PASS
  R3 (div by zero)          → T3      →  2 tests  ✓ PASS
  R4 (REPL loop)            → T4      →  3 tests  ✓ PASS
  R5 (exit on quit)         → T4      →  1 test   ✓ PASS
  R6 (malformed input)      → T1      →  2 tests  ✓ PASS

Items:
  T1 Tokenizer    ✓ covered
  T2 Parser       ✓ covered
  T3 Evaluator    ✓ covered
  T4 REPL loop    ✓ covered

Skipped:
  (none)

Test suite: 24/24 passing
Traceability: 6/6 requirements covered
```

4. If any items were skipped, they appear in a "Skipped" section with the reason.
5. If any requirements have no passing tests, they are flagged: `⚠ R7: no passing tests`.

**Output:** Console report.

**Gate:** Full suite green + all `planned` requirements have passing tests.

---

## 5. Data Model (SQLite)

```sql
-- Requirements (from the spec)
CREATE TABLE requirements (
    id                  TEXT PRIMARY KEY,       -- "R1", "R2", ...
    text                TEXT NOT NULL,          -- requirement statement
    acceptance_criteria TEXT NOT NULL,          -- given/when/then, structured
    state               TEXT NOT NULL DEFAULT 'planned'
                        CHECK (state IN ('planned', 'not-planned', 'covered'))
);

-- WBS Items
CREATE TABLE items (
    id          TEXT PRIMARY KEY,               -- "T1", "T2", ...
    title       TEXT NOT NULL,
    state       TEXT NOT NULL DEFAULT 'planned'
                CHECK (state IN ('planned', 'in-progress', 'covered', 'not-planned')),
    created_at  TEXT NOT NULL DEFAULT (datetime('now'))
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
```

**Location:** `<project-dir>/.agentic/store.db`

---

## 6. Agent Behavior

### 6.1 The Agent

A single hardcoded agent (the "Worker"). No subagents, no YAML graph, no skills.

**Capabilities (tools the agent can use):**

- **llama-cpp inference:** Send prompts to the local llama-cpp server, receive completions.
- **File read:** Read a file from the project directory.
- **File write:** Create or overwrite a file in the project directory.
- **File edit:** Modify a specific portion of a file.
- **Terminal:** Run a command in the project directory, capture stdout/stderr.
- **Git:** Run git commands (add, commit, status, log).
- **SQLite:** Query and update the store.

**Agent loop (per stage):**

```
while not done:
    prompt = build_prompt(context, current_task)
    response = llama_cpp(prompt)
    action = parse_action(response)
    result = execute(action)
    context.append(result)
```

The agent uses a **tool-calling loop**: it receives a prompt, decides on an action (write file, run command, ask user, etc.), executes it, observes the result, and repeats until the stage is complete.

### 6.2 Agent Instructions (hardcoded)

The agent's standing instructions are a hardcoded prompt (not a file, not a skill). Key rules:

1. **Acceptance criteria are the oracle.** Write tests from the spec's AC, not from your implementation.
2. **Full suite always runs.** After every item, run ALL tests.
3. **Commit after every item.** One commit per item.
4. **Never modify existing tests.** If a new test conflicts with an old one, fix the implementation.
5. **Bounded retries.** If tests fail after 3 attempts, skip the item and continue.
6. **Ask when ambiguous.** If the spec is silent on a detail, make a reasonable assumption and note it in the console output.
7. **Never block.** If you can't proceed, skip and continue.

### 6.3 Prompt Structure

Each stage has a prompt template:

**Stage 1 (Spec):**

```
You are a requirements analyst. The user wants: "{goal}".
Ask clarifying questions to understand the full scope.
When you have enough, draft a spec with requirements and acceptance criteria.
Format: R1. <text> | AC: given <X>, when <Y>, then <Z>
```

**Stage 2 (WBS):**

```
You are a project planner. Here are the requirements:
{requirements from SQLite}
Break them into work items. Each item must:
- Cover at least one requirement
- Have a clear title
- Declare dependencies (what must exist first)
Ensure: every requirement is covered, no circular dependencies.
Format: T1. <title> | covers: R1, R2 | depends: —
```

**Stage 3 (TDD, per item):**

```
You are an implementer. Current item: {item.title}
Requirements: {item's requirements + acceptance criteria}
Existing code: {relevant files}
Write tests FIRST from the acceptance criteria. Then implement.
Run the full test suite. All tests must pass.
```

**Stage 4 (Delivery):**

```
Generate the delivery report from the SQLite store.
Run the full test suite. Print the traceability matrix.
```

---

## 7. CLI Interface

```
$ agentic spec "make a calculator"
  → Interactive spec authoring (Q&A with the agent)
  → Draft spec printed
  → User confirms

$ agentic wbs
  → WBS generated and printed
  → User confirms

$ agentic implement
  → TDD loop runs (item by item)
  → Progress printed per item

$ agentic deliver
  → Final test run + traceability matrix printed

$ agentic status
  → Current state: which items are done, in-progress, skipped
  → Test suite status

$ agentic resume
  → Resume from where the last run left off (based on SQLite state)
```

**Single-command mode:**

```
$ agentic run "make a calculator"
  → Runs all 4 stages in sequence
  → Pauses at Stage 1 (user confirms spec) and Stage 2 (user confirms WBS)
  → Stages 3 and 4 run autonomously
```

---

## 8. Non-Functional Requirements

- **Language:** TypeScript, Node.js 20+
- **Inference:** llama-cpp server on `localhost:8080` (user starts it separately). The CLI connects via HTTP.
- **Performance:** The CLI must not block the terminal during inference (stream output as tokens arrive).
- **Resumability:** If the process is interrupted, `agentic resume` picks up from the last completed item (based on SQLite state + git log).
- **Idempotency:** Re-running a stage should not duplicate data. (e.g., re-running WBS updates existing items, doesn't create duplicates.)
- **Error handling:** If llama-cpp is unreachable, the CLI prints a clear error and exits. If a git command fails, the CLI prints the error and stops.
- **Project scoping:** All file operations are scoped to the project directory. The agent cannot read/write outside it.

---

## 9. Acceptance Criteria (for the MVP itself)

The MVP is complete when:

1. `agentic spec "make a calculator"` produces a structured spec with ≥5 requirements, each with testable acceptance criteria, stored in SQLite.
2. `agentic wbs` produces a WBS with full coverage (every requirement mapped) and an acyclic dependency graph, stored in SQLite.
3. `agentic implement` processes all items in topological order, writing tests from AC before implementation, running the full suite after each item, and committing to git.
4. `agentic deliver` prints a traceability matrix showing every requirement → items → tests → pass/fail.
5. The produced calculator program works: it handles +, -, \*, /, precedence, division by zero, REPL, and exit.
6. All tests pass.
7. The traceability matrix shows 100% coverage (no requirement without a passing test).
8. `agentic resume` after an interruption picks up where it left off.
9. The CLI streams agent output in real-time (no long silent pauses).
10. The agent never blocks on the user during Stages 3 and 4.

---

## 10. Out of Scope (Post-MVP)

These are explicitly deferred. They are in the full `SPECIFICATION.md` but NOT in this MVP:

- Analyzer agent (architecture review, scored improvements)
- Explore agent (context-isolated search)
- Declarative agent system (YAML graph + instruction files)
- Skills system (SKILL.md auto-discovery)
- Tiered memory (session/repo/user)
- Assumption ledger + deferral queue (structured)
- Architecture doc maintenance
- Web UI (SolidJS SPA)
- WebSocket real-time streams
- MAC address allowlist / LAN access
- Cascading deferrals
- Rework loop (rejected assumptions → re-implement)
- Multi-project support
- Provider abstraction (cloud APIs)
- Parallel item execution
