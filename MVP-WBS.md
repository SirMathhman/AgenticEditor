# MVP Work Breakdown Structure

## Requirements

Derived from `MVP-SPECIFICATION.md`. Each requirement has testable acceptance criteria.

| ID  | Requirement             | Acceptance Criteria                                                                                                                                               |
| --- | ----------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| R1  | Project scaffolding     | `npm run build` produces a working TypeScript CLI binary. `npm test` runs the test suite.                                                                         |
| R2  | SQLite store            | Can insert, query, and update rows in `requirements`, `items`, `item_requirements`, `item_dependencies` tables. DB file created at `<project>/.agentic/store.db`. |
| R3  | llama-cpp client        | Can send a prompt to `localhost:8080` and receive a streamed completion. Handles connection errors gracefully.                                                    |
| R4  | File I/O tools          | Can read, write, and edit files within the project directory. Cannot read/write outside it (path guard).                                                          |
| R5  | Terminal execution      | Can run a shell command in the project directory and capture stdout/stderr. Handles non-zero exit codes.                                                          |
| R6  | Git integration         | Can `git add`, `git commit`, `git status`, `git log` in the project directory.                                                                                    |
| R7  | Agent loop              | Agent executes a tool-calling loop: build prompt → call llama-cpp → parse action → execute tool → append result → repeat until done.                              |
| R8  | Tool-calling protocol   | Agent can invoke: file_read, file_write, file_edit, terminal, git, sqlite_query, sqlite_update, ask_user, done. Each returns a structured result.                 |
| R9  | Prompt templates        | Four templates exist: spec, WBS, TDD (per-item), delivery. Each produces a valid prompt for its stage.                                                            |
| R10 | Agent instructions      | Hardcoded rules: AC is oracle, full suite per item, commit per item, bounded retries (3), never modify existing tests, skip on failure, never block.              |
| R11 | Interactive Q&A         | Agent asks clarifying questions one at a time (or batched). User types answers. Agent continues until it has enough to draft.                                     |
| R12 | Structured spec output  | Agent produces requirements with `id`, `text`, `acceptance_criteria` (given/when/then). Printed to console for review.                                            |
| R13 | Spec persistence        | On user confirmation, requirements are written to the `requirements` table in SQLite.                                                                             |
| R14 | Spec confirmation loop  | User can: `y` (confirm), `edit` (revise), `n` (restart). Agent loops until confirmed.                                                                             |
| R15 | WBS generation          | Agent breaks requirements into items with `id`, `title`, `requirement_ids`, `depends_on`. Printed to console.                                                     |
| R16 | Coverage check          | Every requirement has ≥1 item. If not, agent creates the missing item.                                                                                            |
| R17 | Acyclicity check        | Topological sort succeeds. If a cycle exists, agent restructures.                                                                                                 |
| R18 | WBS persistence         | On user confirmation, items + mappings + dependencies are written to SQLite.                                                                                      |
| R19 | WBS confirmation loop   | User can: `y` (confirm), `edit` (revise), `n` (regenerate). Agent loops until confirmed.                                                                          |
| R20 | Topological ordering    | Items are processed in topological order (dependencies first).                                                                                                    |
| R21 | Test generation from AC | Tests are written BEFORE implementation. Test names reference the requirement (e.g., `test_R1_addition`). Tests assert the AC, not the implementation.            |
| R22 | Implementation          | Agent writes code to satisfy the tests. Files created/modified in the project directory.                                                                          |
| R23 | Full test suite         | After implementation, the ENTIRE test suite runs (not just new tests). All must pass.                                                                             |
| R24 | Bounded retry           | If tests fail, agent retries up to 3 times. After 3 failures, the item is marked `not-planned` and skipped.                                                       |
| R25 | Git commit per item     | After tests pass, agent commits: `git add -A && git commit -m "T<N>: <title> (R<X>, R<Y>)"`.                                                                      |
| R26 | Skip + cascade          | If an item is skipped, all items that depend on it are also skipped (with a note).                                                                                |
| R27 | Progress output         | Per item, agent prints: `→ T<N>: <title>` (start), `✓ T<N>: <title> — N tests passing` (success), or `✗ T<N>: <title> — skipped` (failure).                       |
| R28 | Final test run          | At delivery, the full test suite runs one final time.                                                                                                             |
| R29 | Traceability matrix     | For each requirement: which items cover it, which tests exist, pass/fail status.                                                                                  |
| R30 | Delivery report         | Console output: requirements table, items table, skipped items, test suite summary, traceability summary.                                                         |
| R31 | CLI commands            | `spec`, `wbs`, `implement`, `deliver`, `status`, `resume`, `run` all work. `run` executes all 4 stages in sequence.                                               |
| R32 | Streaming output        | Agent output streams to the console in real-time (no long silent pauses during inference).                                                                        |
| R33 | Resumability            | `agentic resume` picks up from the last completed item (based on SQLite state).                                                                                   |
| R34 | Idempotency             | Re-running a stage does not duplicate data. WBS re-run updates existing items.                                                                                    |
| R35 | Error handling          | llama-cpp unreachable → clear error + exit. Git failure → clear error + stop. File I/O error → clear error + stop.                                                |
| R36 | Calculator example      | `agentic run "make a calculator"` produces a working calculator: +, -, \*, /, precedence, div-by-zero, REPL, exit. 100% traceability. All tests pass.             |

---

## Work Items

### Phase 1: Foundation

| ID  | Title               | Covers | Depends | AC (testable)                                                                                                                                        |
| --- | ------------------- | ------ | ------- | ---------------------------------------------------------------------------------------------------------------------------------------------------- |
| T1  | Project scaffolding | R1     | —       | `npm run build` succeeds. `npm test` runs. Directory structure: `src/`, `tests/`, `package.json`, `tsconfig.json`.                                   |
| T2  | SQLite store        | R2     | T1      | Can create tables, insert a requirement, query it back, insert an item, map it to the requirement, add a dependency. DB file at `.agentic/store.db`. |
| T3  | llama-cpp client    | R3     | T1      | Can POST a prompt to `localhost:8080/v1/completions` and receive a streamed response. If server is down, throws a clear error.                       |
| T4  | File I/O tools      | R4     | T1      | Can read a file, write a new file, edit a portion of a file. Attempting to read `/etc/passwd` (outside project) throws a path-guard error.           |
| T5  | Terminal execution  | R5     | T1      | Can run `echo hello` and capture `hello\n`. Can run a failing command and capture the non-zero exit code + stderr.                                   |
| T6  | Git integration     | R6     | T1      | Can `git init` a temp dir, `git add` a file, `git commit` it, `git log` shows the commit.                                                            |

### Phase 2: Agent Core

| ID  | Title                           | Covers  | Depends        | AC (testable)                                                                                                                                                                                                  |
| --- | ------------------------------- | ------- | -------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| T7  | Agent loop + tool protocol      | R7, R8  | T3, T4, T5, T6 | Given a simple task ("create a file with 'hello'"), the agent loop: calls llama-cpp → parses `file_write` action → executes it → appends result → calls llama-cpp again → parses `done` → stops.               |
| T8  | Prompt templates + instructions | R9, R10 | T1             | Four template functions exist: `specPrompt(goal)`, `wbsPrompt(requirements)`, `tddPrompt(item, requirements, context)`, `deliveryPrompt()`. Each returns a non-empty string containing the agent instructions. |

### Phase 3: Pipeline Stages

| ID  | Title                                         | Covers        | Depends     | AC (testable)                                                                                                                                                                                                                             |
| --- | --------------------------------------------- | ------------- | ----------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| T9  | Stage 1: Spec authoring                       | R11–R14       | T7, T8, T2  | Given goal "make a calculator": agent asks ≥3 clarifying questions, drafts a spec with ≥5 requirements (each with given/when/then AC), prints it, accepts `y`/`edit`/`n`, and on `y` writes requirements to SQLite.                       |
| T10 | Stage 2: WBS generation                       | R15–R19       | T9, T2      | Given the requirements from T9: agent produces items with requirement mappings + dependencies, verifies coverage (all R's mapped) + acyclicity (topo sort succeeds), prints the WBS, accepts `y`/`edit`/`n`, and on `y` writes to SQLite. |
| T11 | Stage 3a: Topological order + iteration       | R20, R26      | T10, T2     | Given a WBS with deps T1→T2→T3: items are processed in order T1, T2, T3. If T2 is skipped, T3 is also skipped (with note).                                                                                                                |
| T12 | Stage 3b: Test generation from AC             | R21           | T11, T7, T8 | Given item T3 (Evaluator, covers R1, R3): agent writes test file with tests named `test_R1_*` and `test_R3_*` BEFORE writing implementation. Tests assert the AC values (e.g., `expect(eval("2+3")).toBe(5)`).                            |
| T13 | Stage 3c: Implementation + full suite + retry | R22, R23, R24 | T12, T4, T5 | Agent writes implementation code. Runs full test suite. If tests fail, retries (up to 3). After 3 failures, marks item `not-planned`. If tests pass, item is `covered`.                                                                   |
| T14 | Stage 3d: Git commit + progress               | R25, R27      | T13, T6     | After tests pass: `git commit -m "T3: Evaluator (R1, R3)"` is created. Console shows `→ T3: Evaluator` then `✓ T3: Evaluator — 5 tests passing`.                                                                                          |
| T15 | Stage 4: Delivery report                      | R28–R30       | T14, T2, T5 | Runs full suite. Prints traceability matrix (each R → items → tests → ✓/✗). Prints items table (covered/skipped). Prints summary: "N/N requirements covered, M/M tests passing".                                                          |

### Phase 4: CLI

| ID  | Title                      | Covers   | Depends           | AC (testable)                                                                                                                                                                                |
| --- | -------------------------- | -------- | ----------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| T16 | CLI commands               | R31      | T15, T9, T10, T14 | `agentic spec "goal"`, `agentic wbs`, `agentic implement`, `agentic deliver`, `agentic status`, `agentic resume`, `agentic run "goal"` all execute the correct stage(s). `run` chains all 4. |
| T17 | Streaming + error handling | R32, R35 | T16, T3, T6       | Agent output appears token-by-token (not all at once). If llama-cpp is down: "Error: llama-cpp server unreachable at localhost:8080" + exit code 1. If git fails: clear error + stop.        |
| T18 | Resumability + idempotency | R33, R34 | T17, T2           | After T3 completes and process is killed: `agentic resume` starts at T4. Re-running `agentic wbs` updates existing items (no duplicates in SQLite).                                          |

### Phase 5: End-to-End

| ID  | Title              | Covers | Depends | AC (testable)                                                                                                                                                                                                                                               |
| --- | ------------------ | ------ | ------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| T19 | Calculator example | R36    | T18     | `agentic run "make a calculator"` in a fresh directory produces: a working calculator (handles +, -, \*, /, precedence, div-by-zero, REPL, exit), all tests passing, 100% traceability (every R has ≥1 passing test), git history with one commit per item. |

---

## Dependency Graph

```
T1 (scaffolding)
├── T2 (SQLite)
├── T3 (llama-cpp)
├── T4 (File I/O)
├── T5 (Terminal)
├── T6 (Git)
└── T8 (Prompts)
         │
         ▼
T7 (Agent loop) ←── T3, T4, T5, T6
         │
         ▼
T9 (Stage 1: Spec) ←── T7, T8, T2
         │
         ▼
T10 (Stage 2: WBS) ←── T9, T2
         │
         ▼
T11 (Topo order) ←── T10, T2
         │
         ▼
T12 (Test gen) ←── T11, T7, T8
         │
         ▼
T13 (Implement + suite) ←── T12, T4, T5
         │
         ▼
T14 (Commit + progress) ←── T13, T6
         │
         ▼
T15 (Delivery) ←── T14, T2, T5
         │
         ▼
T16 (CLI commands) ←── T15, T9, T10, T14
         │
         ▼
T17 (Streaming + errors) ←── T16, T3, T6
         │
         ▼
T18 (Resume + idempotency) ←── T17, T2
         │
         ▼
T19 (Calculator E2E) ←── T18
```

**Critical path:** T1 → T3 → T7 → T9 → T10 → T11 → T12 → T13 → T14 → T15 → T16 → T17 → T18 → T19

**Parallelizable (Phase 1):** T2, T3, T4, T5, T6, T8 can all be built in parallel after T1.

---

## Verification

### Coverage

| Requirement | Items |
| ----------- | ----- |
| R1          | T1    |
| R2          | T2    |
| R3          | T3    |
| R4          | T4    |
| R5          | T5    |
| R6          | T6    |
| R7          | T7    |
| R8          | T7    |
| R9          | T8    |
| R10         | T8    |
| R11         | T9    |
| R12         | T9    |
| R13         | T9    |
| R14         | T9    |
| R15         | T10   |
| R16         | T10   |
| R17         | T10   |
| R18         | T10   |
| R19         | T10   |
| R20         | T11   |
| R21         | T12   |
| R22         | T13   |
| R23         | T13   |
| R24         | T13   |
| R25         | T14   |
| R26         | T11   |
| R27         | T14   |
| R28         | T15   |
| R29         | T15   |
| R30         | T15   |
| R31         | T16   |
| R32         | T17   |
| R33         | T18   |
| R34         | T18   |
| R35         | T17   |
| R36         | T19   |

**36/36 requirements covered. ✓**

### Acyclicity

Topological sort: T1 → {T2, T3, T4, T5, T6, T8} → T7 → T9 → T10 → T11 → T12 → T13 → T14 → T15 → T16 → T17 → T18 → T19

**No cycles. ✓**
