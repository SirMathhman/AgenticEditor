//! The agent's tool registry: the set of tools the chat agent may call, each
//! with its schema and executor bound together so the two can never drift
//! apart. File and directory tools wrap the root-scoped operations in
//! `tree.rs`; `get_local_time` is self-contained. This module is tauri-free.

use super::errors::AppError;
use std::io::Read;
use std::path::Path;

/// A tool executor: runs a tool given its arguments as a JSON string. A boxed
/// closure (not a plain fn) so file/dir tools can capture the project root.
pub(crate) type ToolExecutor = Box<dyn Fn(&str) -> Result<String, AppError> + Send + Sync>;

/// A tool the agent may call: its name and its executor. The name,
/// description, and argument schema live in [`ToolMeta`] (the static
/// metadata); this struct pairs each with the executor that runs it.
pub(crate) struct Tool {
    /// The tool's name (e.g. `get_local_time`), as sent to and from the model.
    pub name: &'static str,
    /// Executes the tool with its arguments as a JSON string.
    execute: ToolExecutor,
}

/// Builds a tool executor that captures its own owned clone of the project
/// root, so the returned `Vec<Tool>` owns its executors (a captured `&Path`
/// would not outlive it).
fn tool_fn(
    root: &std::sync::Arc<Path>,
    f: impl Fn(&Path, &str) -> Result<String, AppError> + Send + Sync + 'static,
) -> ToolExecutor {
    let root = root.clone();
    Box::new(move |args| f(&root, args))
}

/// A tool's static metadata: its name, description, and argument schema. This
/// is everything that does not depend on the project root, so it can be read
/// (for the UI's tool list and the request schema) without building any
/// executors.
pub struct ToolMeta {
    /// The tool's name (e.g. `get_local_time`), as sent to and from the model.
    pub name: &'static str,
    /// A human/model-facing description of what the tool does.
    pub description: &'static str,
    /// The JSON schema of the tool's arguments (OpenAI-compatible).
    pub parameters: serde_json::Value,
}

/// The static metadata for every tool the agent may call, in registry order.
/// This is the single source of truth for the tool surface: [`tools`],
/// [`tool_info`], and the request schema all derive from it, so the list the
/// user sees can never drift from the tools the agent actually has.
pub fn tool_metas() -> Vec<ToolMeta> {
    vec![
        ToolMeta {
            name: "get_local_time",
            description: "Get the current local date and time on the user's \
                          machine, in the user's local timezone.",
            parameters: serde_json::json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
        },
        ToolMeta {
            name: "list_dir",
            description: "List the files and subdirectories in a directory of \
                          the open project. Directories are marked with a \
                          trailing '/'. Use an empty path for the project root.",
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Directory path relative to the project root (empty for the root)."
                    }
                },
                "required": ["path"],
                "additionalProperties": false
            }),
        },
        ToolMeta {
            name: "read_file",
            description: "Read the full text contents of a file in the open \
                          project, given its path relative to the project root.",
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "File path relative to the project root."
                    }
                },
                "required": ["path"],
                "additionalProperties": false
            }),
        },
        ToolMeta {
            name: "write_file",
            description: "Write text to a file in the open project, creating \
                          it (and nothing else) if missing or overwriting it if \
                          present. Path is relative to the project root.",
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "File path relative to the project root."
                    },
                    "contents": {
                        "type": "string",
                        "description": "The full text to write to the file."
                    }
                },
                "required": ["path", "contents"],
                "additionalProperties": false
            }),
        },
        ToolMeta {
            name: "create_dir",
            description: "Create a directory (and any missing parents) in the \
                          open project. Path is relative to the project root.",
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Directory path relative to the project root."
                    }
                },
                "required": ["path"],
                "additionalProperties": false
            }),
        },
        ToolMeta {
            name: "delete",
            description: "Delete a file or directory (recursively) from the \
                          open project. Path is relative to the project root.",
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "File or directory path relative to the project root."
                    }
                },
                "required": ["path"],
                "additionalProperties": false
            }),
        },
        ToolMeta {
            name: "run_command",
            description: "Run a PowerShell command in the open project's root \
                          folder and return its exit code, stdout, and stderr. \
                          The command runs non-interactively and is killed if it \
                          exceeds the timeout (default 30 seconds). Use it to run \
                          builds, tests, git, and other CLI tools.",
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "The PowerShell command line to run."
                    },
                    "timeout": {
                        "type": "integer",
                        "description": "Optional maximum runtime in seconds (default 30)."
                    }
                },
                "required": ["command"],
                "additionalProperties": false
            }),
        },
        ToolMeta {
            name: "memory",
            description: "Read and edit the agent's persistent project memory: \
                          markdown/text files that survive across chat sessions. \
                          Actions: 'view' (read a file or list a directory; an \
                          empty path lists the memory root), 'create' (create a \
                          new file with 'contents'; fails if it already exists), \
                          'str_replace' (replace a unique 'old_str' with \
                          'new_str'), 'insert' (insert 'insert_text' at the \
                          0-based 'insert_line'), 'delete' (remove a file or \
                          directory), 'rename' (move or rename 'path' to \
                          'new_path'). Paths are relative to the memory root. \
                          Use it to remember decisions, conventions, and context \
                          that should persist between conversations.",
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["view", "create", "str_replace", "insert", "delete", "rename"],
                        "description": "The memory operation to perform."
                    },
                    "path": {
                        "type": "string",
                        "description": "File or directory path relative to the memory root. Required for view, create, str_replace, insert, and delete; the source path for rename. An empty path with 'view' lists the memory root."
                    },
                    "contents": {
                        "type": "string",
                        "description": "The full text to write. Required for create."
                    },
                    "old_str": {
                        "type": "string",
                        "description": "The exact text to replace; it must appear exactly once. Required for str_replace."
                    },
                    "new_str": {
                        "type": "string",
                        "description": "The replacement text. Required for str_replace."
                    },
                    "insert_line": {
                        "type": "integer",
                        "description": "The 0-based line number to insert at. Required for insert."
                    },
                    "insert_text": {
                        "type": "string",
                        "description": "The text to insert. Required for insert."
                    },
                    "new_path": {
                        "type": "string",
                        "description": "The destination path relative to the memory root. Required for rename."
                    }
                },
                "required": ["action"],
                "additionalProperties": false
            }),
        },
    ]
}

/// The executor for a tool, by name. File and directory tools are scoped to
/// `root` (the open project folder); the `memory` tool is scoped to
/// `memory_root` (the project's memory directory); `get_local_time` ignores
/// both.
fn executor_for(
    name: &str,
    root: &std::sync::Arc<Path>,
    memory_root: &std::sync::Arc<Path>,
) -> ToolExecutor {
    match name {
        "get_local_time" => Box::new(|args| {
            // Takes no parameters; reject any the model might invent.
            let value: serde_json::Value = serde_json::from_str(args)
                .map_err(|e| AppError::Tool(format!("invalid arguments: {e}")))?;
            if !value.is_object() {
                return Err(AppError::Tool(
                    "get_local_time takes no arguments".to_string(),
                ));
            }
            Ok(local_time())
        }),
        "list_dir" => tool_fn(root, |root, args| {
            let path = arg_str(args, "path")?;
            super::tree::list_dir_at(root, &path)
        }),
        "read_file" => tool_fn(root, |root, args| {
            let path = arg_str(args, "path")?;
            super::tree::read_file_at(root, &path)
        }),
        "write_file" => tool_fn(root, |root, args| {
            let path = arg_str(args, "path")?;
            let contents = arg_str(args, "contents")?;
            super::tree::write_file_at(root, &path, &contents)?;
            Ok(format!("wrote {} bytes to {path}", contents.len()))
        }),
        "create_dir" => tool_fn(root, |root, args| {
            let path = arg_str(args, "path")?;
            super::tree::create_dir_at(root, &path)
        }),
        "delete" => tool_fn(root, |root, args| {
            let path = arg_str(args, "path")?;
            super::tree::delete_at(root, &path)
        }),
        "run_command" => tool_fn(root, run_command),
        "memory" => {
            let memory_root = memory_root.clone();
            Box::new(move |args| memory_tool(&memory_root, args))
        }
        // Every name in `tool_metas` has an executor above; reaching here means
        // the two lists have drifted, which is a programming error.
        other => panic!("no executor registered for tool: {other}"),
    }
}

/// The registry of all tools the agent may call: the static metadata from
/// [`tool_metas`] with an executor attached to each, scoped to `root` (the
/// project folder) and `memory_root` (the project's memory directory).
pub(crate) fn tools(root: &Path, memory_root: &Path) -> Vec<Tool> {
    let root: std::sync::Arc<Path> = std::sync::Arc::from(root.to_path_buf());
    let memory_root: std::sync::Arc<Path> = std::sync::Arc::from(memory_root.to_path_buf());
    tool_metas()
        .into_iter()
        .map(|meta| Tool {
            name: meta.name,
            execute: executor_for(meta.name, &root, &memory_root),
        })
        .collect()
}

/// Parses a tool's arguments JSON and returns the named string field, erroring
/// when the arguments are malformed or the field is missing.
fn arg_str(arguments: &str, field: &str) -> Result<String, AppError> {
    let value: serde_json::Value = serde_json::from_str(arguments)
        .map_err(|e| AppError::Tool(format!("invalid arguments: {e}")))?;
    value
        .get(field)
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .ok_or_else(|| AppError::Tool(format!("missing or non-string argument: {field}")))
}

/// Parses a tool's arguments JSON and returns the named string field, or
/// `None` when the field is absent or not a string. Used for optional fields.
fn arg_str_opt(arguments: &str, field: &str) -> Result<Option<String>, AppError> {
    let value: serde_json::Value = serde_json::from_str(arguments)
        .map_err(|e| AppError::Tool(format!("invalid arguments: {e}")))?;
    Ok(value
        .get(field)
        .and_then(|v| v.as_str())
        .map(str::to_string))
}

/// Dispatches a `memory` tool call to the matching operation in `memory.rs`.
/// The `action` field selects the operation; the other fields are required
/// only for the actions that use them.
fn memory_tool(memory_root: &Path, args: &str) -> Result<String, AppError> {
    let action = arg_str(args, "action")?;
    match action.as_str() {
        "view" => {
            let path = arg_str_opt(args, "path")?.unwrap_or_default();
            super::memory::view(memory_root, &path)
        }
        "create" => {
            let path = arg_str(args, "path")?;
            let contents = arg_str(args, "contents")?;
            super::memory::create(memory_root, &path, &contents)
        }
        "str_replace" => {
            let path = arg_str(args, "path")?;
            let old = arg_str(args, "old_str")?;
            let new = arg_str(args, "new_str")?;
            super::memory::str_replace(memory_root, &path, &old, &new)
        }
        "insert" => {
            let path = arg_str(args, "path")?;
            let line = arg_u64(args, "insert_line")?
                .ok_or_else(|| AppError::Tool("insert requires an insert_line".to_string()))?;
            let text = arg_str(args, "insert_text")?;
            super::memory::insert(memory_root, &path, line as usize, &text)
        }
        "delete" => {
            let path = arg_str(args, "path")?;
            super::memory::delete(memory_root, &path)
        }
        "rename" => {
            let old = arg_str(args, "path")?;
            let new = arg_str(args, "new_path")?;
            super::memory::rename(memory_root, &old, &new)
        }
        other => Err(AppError::Tool(format!("unknown memory action: {other}"))),
    }
}

/// Executes a tool call by name. Unknown tool names are an error rather than
/// a silent no-op, so a model hallucinating a tool name surfaces as a visible
/// failure instead of a wrong answer.
pub fn execute_tool(
    root: &Path,
    memory_root: &Path,
    name: &str,
    arguments: &str,
) -> Result<String, AppError> {
    match tools(root, memory_root)
        .into_iter()
        .find(|t| t.name == name)
    {
        Some(tool) => {
            let execute = tool.execute;
            execute(arguments)
        }
        None => Err(AppError::Tool(format!("unknown tool: {name}"))),
    }
}

/// A tool's name and description, as shown to the user. Derived from the
/// registry so the list can never drift from the tools the agent actually has.
#[derive(serde::Serialize, Clone)]
pub struct ToolInfo {
    /// The tool's name (e.g. `get_local_time`).
    pub name: String,
    /// A human-facing description of what the tool does.
    pub description: String,
}

/// The names and descriptions of every tool the agent may call, in registry
/// order. Derived from the static metadata, so no executors are built and no
/// root is needed.
pub fn tool_info() -> Vec<ToolInfo> {
    tool_metas()
        .into_iter()
        .map(|m| ToolInfo {
            name: m.name.to_string(),
            description: m.description.to_string(),
        })
        .collect()
}

/// The current local date and time, formatted for display.
fn local_time() -> String {
    chrono::Local::now()
        .format("%A, %B %d, %Y %H:%M:%S %z")
        .to_string()
}

/// Default maximum runtime for a `run_command` call, in seconds.
const DEFAULT_COMMAND_TIMEOUT_SECS: u64 = 30;
/// Hard cap on how much stdout/stderr is captured per stream, so a chatty
/// command cannot blow up the tool result (and the model's context).
const MAX_COMMAND_OUTPUT_BYTES: usize = 100_000;

/// Runs a PowerShell command in the project root and returns its exit code,
/// stdout, and stderr. The command runs non-interactively and is killed if it
/// exceeds the timeout. This is a blocking call (it waits for the child), so
/// it must run on a blocking thread — the tool loop already does.
fn run_command(root: &Path, args: &str) -> Result<String, AppError> {
    let command = arg_str(args, "command")?;
    let timeout_secs = arg_u64(args, "timeout")?.unwrap_or(DEFAULT_COMMAND_TIMEOUT_SECS);

    let mut child = std::process::Command::new("pwsh")
        .arg("-NoProfile")
        .arg("-Command")
        .arg(&command)
        .current_dir(root)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| AppError::Tool(format!("failed to start pwsh: {e}")))?;

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(timeout_secs);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let stdout = read_capped(&mut child.stdout.take().expect("stdout piped"));
                let stderr = read_capped(&mut child.stderr.take().expect("stderr piped"));
                return Ok(format_command_output(&status, &stdout, &stderr));
            }
            Ok(None) => {
                if std::time::Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Ok(format!(
                        "command timed out after {timeout_secs}s and was killed: {command}"
                    ));
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            Err(e) => return Err(AppError::Tool(format!("failed to wait for command: {e}"))),
        }
    }
}

/// Reads a stream to EOF, capping at `MAX_COMMAND_OUTPUT_BYTES` so a very
/// chatty command cannot produce an unbounded tool result.
fn read_capped(reader: &mut dyn Read) -> String {
    let mut buf: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        match reader.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                buf.extend_from_slice(&chunk[..n]);
                if buf.len() >= MAX_COMMAND_OUTPUT_BYTES {
                    buf.truncate(MAX_COMMAND_OUTPUT_BYTES);
                    break;
                }
            }
            Err(_) => break,
        }
    }
    String::from_utf8_lossy(&buf).into_owned()
}

/// Formats a finished command's exit code and captured output for the model.
fn format_command_output(status: &std::process::ExitStatus, stdout: &str, stderr: &str) -> String {
    let mut out = format!("exit code: {}\n", status.code().unwrap_or(-1));
    if !stdout.is_empty() {
        out.push_str(&format!("stdout:\n{stdout}\n"));
    }
    if !stderr.is_empty() {
        out.push_str(&format!("stderr:\n{stderr}"));
    }
    if stdout.is_empty() && stderr.is_empty() {
        out.push_str("(no output)");
    }
    out
}

/// Parses a tool's arguments JSON and returns the named unsigned-integer field
/// as `Some`, or `None` when the field is absent or not a number.
fn arg_u64(arguments: &str, field: &str) -> Result<Option<u64>, AppError> {
    let value: serde_json::Value = serde_json::from_str(arguments)
        .map_err(|e| AppError::Tool(format!("invalid arguments: {e}")))?;
    Ok(value.get(field).and_then(|v| v.as_u64()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn execute_tool_get_local_time_returns_a_date() {
        let dir = tempfile::tempdir().unwrap();
        let mem = tempfile::tempdir().unwrap();
        let result = execute_tool(dir.path(), mem.path(), "get_local_time", "{}").unwrap();
        // The format is "Weekday, Month DD, YYYY HH:MM:SS ±ZZZZ" — check the
        // shape rather than the exact value, which changes every second.
        assert!(result.len() >= 24, "unexpected result: {result}");
        assert!(result.contains(','), "unexpected result: {result}");
    }

    #[test]
    fn execute_tool_rejects_non_object_arguments() {
        let dir = tempfile::tempdir().unwrap();
        let mem = tempfile::tempdir().unwrap();
        assert!(execute_tool(dir.path(), mem.path(), "get_local_time", "[1, 2]").is_err());
    }

    #[test]
    fn execute_tool_unknown_name_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let mem = tempfile::tempdir().unwrap();
        assert!(execute_tool(dir.path(), mem.path(), "no_such_tool", "{}").is_err());
    }

    #[test]
    fn execute_tool_list_dir_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let mem = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("sub")).unwrap();
        std::fs::write(dir.path().join("a.txt"), "x").unwrap();

        let result = execute_tool(dir.path(), mem.path(), "list_dir", r#"{"path": ""}"#).unwrap();
        // Directories sort first and are marked with a trailing slash.
        assert!(result.contains("sub/"), "unexpected: {result}");
        assert!(result.contains("a.txt"), "unexpected: {result}");
    }

    #[test]
    fn execute_tool_write_then_read_file() {
        let dir = tempfile::tempdir().unwrap();
        let mem = tempfile::tempdir().unwrap();
        execute_tool(
            dir.path(),
            mem.path(),
            "write_file",
            r#"{"path": "note.txt", "contents": "hello"}"#,
        )
        .unwrap();
        let result = execute_tool(
            dir.path(),
            mem.path(),
            "read_file",
            r#"{"path": "note.txt"}"#,
        )
        .unwrap();
        assert_eq!(result, "hello");
    }

    #[test]
    fn execute_tool_create_dir_then_delete() {
        let dir = tempfile::tempdir().unwrap();
        let mem = tempfile::tempdir().unwrap();
        execute_tool(
            dir.path(),
            mem.path(),
            "create_dir",
            r#"{"path": "newdir"}"#,
        )
        .unwrap();
        assert!(dir.path().join("newdir").is_dir());
        execute_tool(dir.path(), mem.path(), "delete", r#"{"path": "newdir"}"#).unwrap();
        assert!(!dir.path().join("newdir").exists());
    }

    #[test]
    fn execute_tool_rejects_path_traversal() {
        let dir = tempfile::tempdir().unwrap();
        let mem = tempfile::tempdir().unwrap();
        // A file outside the root must not be readable through the tool.
        let result = execute_tool(
            dir.path(),
            mem.path(),
            "read_file",
            r#"{"path": "../escape.txt"}"#,
        );
        assert!(result.is_err());
    }

    #[test]
    fn execute_tool_run_command_requires_command() {
        let dir = tempfile::tempdir().unwrap();
        let mem = tempfile::tempdir().unwrap();
        assert!(execute_tool(dir.path(), mem.path(), "run_command", r#"{}"#).is_err());
    }

    #[test]
    fn execute_tool_run_command_runs_and_reports_output() {
        let dir = tempfile::tempdir().unwrap();
        let mem = tempfile::tempdir().unwrap();
        // Skip when pwsh is not installed (e.g. a minimal CI image).
        if std::process::Command::new("pwsh")
            .arg("--version")
            .output()
            .is_err()
        {
            return;
        }
        let result = execute_tool(
            dir.path(),
            mem.path(),
            "run_command",
            r#"{"command": "Write-Output hello"}"#,
        )
        .unwrap();
        assert!(result.contains("exit code: 0"), "unexpected: {result}");
        assert!(result.contains("hello"), "unexpected: {result}");
    }

    #[test]
    fn execute_tool_memory_create_then_view() {
        let dir = tempfile::tempdir().unwrap();
        let mem = tempfile::tempdir().unwrap();
        execute_tool(
            dir.path(),
            mem.path(),
            "memory",
            r#"{"action": "create", "path": "notes.md", "contents": "remember this"}"#,
        )
        .unwrap();
        let result = execute_tool(
            dir.path(),
            mem.path(),
            "memory",
            r#"{"action": "view", "path": "notes.md"}"#,
        )
        .unwrap();
        assert_eq!(result, "remember this");
    }

    #[test]
    fn execute_tool_memory_unknown_action_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let mem = tempfile::tempdir().unwrap();
        let result = execute_tool(
            dir.path(),
            mem.path(),
            "memory",
            r#"{"action": "frobnicate"}"#,
        );
        assert!(result.is_err());
    }
}
