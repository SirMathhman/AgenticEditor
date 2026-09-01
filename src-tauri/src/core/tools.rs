//! The agent's tool registry: the set of tools the chat agent may call, each
//! with its schema and executor bound together so the two can never drift
//! apart. File and directory tools wrap the root-scoped operations in
//! `tree.rs`; `get_local_time` is self-contained. This module is tauri-free.

use super::errors::AppError;
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
    ]
}

/// The executor for a tool, by name. File and directory tools are scoped to
/// `root` (the open project folder); `get_local_time` ignores it.
fn executor_for(name: &str, root: &std::sync::Arc<Path>) -> ToolExecutor {
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
        // Every name in `tool_metas` has an executor above; reaching here means
        // the two lists have drifted, which is a programming error.
        other => panic!("no executor registered for tool: {other}"),
    }
}

/// The registry of all tools the agent may call: the static metadata from
/// [`tool_metas`] with an executor attached to each, scoped to `root`.
pub(crate) fn tools(root: &Path) -> Vec<Tool> {
    let root: std::sync::Arc<Path> = std::sync::Arc::from(root.to_path_buf());
    tool_metas()
        .into_iter()
        .map(|meta| Tool {
            name: meta.name,
            execute: executor_for(meta.name, &root),
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

/// Executes a tool call by name. Unknown tool names are an error rather than
/// a silent no-op, so a model hallucinating a tool name surfaces as a visible
/// failure instead of a wrong answer.
pub fn execute_tool(root: &Path, name: &str, arguments: &str) -> Result<String, AppError> {
    match tools(root).into_iter().find(|t| t.name == name) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn execute_tool_get_local_time_returns_a_date() {
        let dir = tempfile::tempdir().unwrap();
        let result = execute_tool(dir.path(), "get_local_time", "{}").unwrap();
        // The format is "Weekday, Month DD, YYYY HH:MM:SS ±ZZZZ" — check the
        // shape rather than the exact value, which changes every second.
        assert!(result.len() >= 24, "unexpected result: {result}");
        assert!(result.contains(','), "unexpected result: {result}");
    }

    #[test]
    fn execute_tool_rejects_non_object_arguments() {
        let dir = tempfile::tempdir().unwrap();
        assert!(execute_tool(dir.path(), "get_local_time", "[1, 2]").is_err());
    }

    #[test]
    fn execute_tool_unknown_name_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        assert!(execute_tool(dir.path(), "no_such_tool", "{}").is_err());
    }

    #[test]
    fn execute_tool_list_dir_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("sub")).unwrap();
        std::fs::write(dir.path().join("a.txt"), "x").unwrap();

        let result = execute_tool(dir.path(), "list_dir", r#"{"path": ""}"#).unwrap();
        // Directories sort first and are marked with a trailing slash.
        assert!(result.contains("sub/"), "unexpected: {result}");
        assert!(result.contains("a.txt"), "unexpected: {result}");
    }

    #[test]
    fn execute_tool_write_then_read_file() {
        let dir = tempfile::tempdir().unwrap();
        execute_tool(
            dir.path(),
            "write_file",
            r#"{"path": "note.txt", "contents": "hello"}"#,
        )
        .unwrap();
        let result = execute_tool(dir.path(), "read_file", r#"{"path": "note.txt"}"#).unwrap();
        assert_eq!(result, "hello");
    }

    #[test]
    fn execute_tool_create_dir_then_delete() {
        let dir = tempfile::tempdir().unwrap();
        execute_tool(dir.path(), "create_dir", r#"{"path": "newdir"}"#).unwrap();
        assert!(dir.path().join("newdir").is_dir());
        execute_tool(dir.path(), "delete", r#"{"path": "newdir"}"#).unwrap();
        assert!(!dir.path().join("newdir").exists());
    }

    #[test]
    fn execute_tool_rejects_path_traversal() {
        let dir = tempfile::tempdir().unwrap();
        // A file outside the root must not be readable through the tool.
        let result = execute_tool(dir.path(), "read_file", r#"{"path": "../escape.txt"}"#);
        assert!(result.is_err());
    }
}
