#![forbid(unsafe_code)]

//! Minimal tool registry, permission routing, and reference tools for Phase 3.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use clawin_core::{
    ClawinError, ClawinResult, PermissionBehavior, PermissionDecision, SessionRuntime, ToolCall,
    ToolKind, ToolResult, ToolSource, ToolSpec,
};
use serde::Deserialize;
use serde_json::{Value, json};

type ToolLoader = fn() -> Box<dyn ToolHandler>;

/// Structured result of a tool execution plus the permission behavior that led to it.
#[derive(Clone, Debug, PartialEq)]
pub struct ToolExecution {
    pub permission_behavior: PermissionBehavior,
    pub result: ToolResult,
}

/// Tool implementation contract used behind lazy registry entries.
pub trait ToolHandler: Send + Sync {
    fn validate(&self, input: &Value) -> ClawinResult<()>;
    fn check_permission(
        &self,
        runtime: &SessionRuntime,
        call: &ToolCall,
    ) -> ClawinResult<PermissionDecision>;
    fn call(&self, runtime: &SessionRuntime, call: &ToolCall) -> ClawinResult<ToolResult>;
}

#[derive(Clone)]
struct RegisteredTool {
    spec: ToolSpec,
    load: ToolLoader,
}

/// Tool registry with stable specs and lazy-loaded handlers.
#[derive(Clone, Default)]
pub struct ToolRegistry {
    tools: Vec<RegisteredTool>,
    lookup: BTreeMap<String, usize>,
}

impl std::fmt::Debug for ToolRegistry {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ToolRegistry")
            .field("tools", &self.tool_specs().collect::<Vec<_>>())
            .finish()
    }
}

impl ToolRegistry {
    /// Create an empty tool registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a lazy-loaded tool.
    pub fn register(&mut self, spec: ToolSpec, load: ToolLoader) {
        let index = self.tools.len();
        self.lookup.insert(spec.name.clone(), index);
        self.tools.push(RegisteredTool { spec, load });
    }

    /// Borrow the tool spec for a given name.
    pub fn spec(&self, name: &str) -> Option<&ToolSpec> {
        self.lookup
            .get(name)
            .and_then(|index| self.tools.get(*index))
            .map(|entry| &entry.spec)
    }

    /// Iterate over registered tool specs.
    pub fn tool_specs(&self) -> impl Iterator<Item = &ToolSpec> {
        self.tools.iter().map(|entry| &entry.spec)
    }

    /// Execute a tool call, returning a structured result and permission behavior.
    pub fn execute(&self, call: ToolCall, runtime: &SessionRuntime) -> ClawinResult<ToolExecution> {
        let handler = self.load(&call.tool_name)?;
        handler.validate(&call.input)?;
        let decision = handler.check_permission(runtime, &call)?;

        match decision.behavior {
            PermissionBehavior::Allow => Ok(ToolExecution {
                permission_behavior: PermissionBehavior::Allow,
                result: handler.call(runtime, &call)?,
            }),
            PermissionBehavior::Ask => Ok(ToolExecution {
                permission_behavior: PermissionBehavior::Ask,
                result: ToolResult::error(
                    &call,
                    json!({
                        "type": "error",
                        "code": "permission_denied",
                        "message": "permission prompt not implemented yet for paths outside the project root",
                        "permission_behavior": PermissionBehavior::Ask.as_str(),
                    }),
                ),
            }),
            PermissionBehavior::Deny => Ok(ToolExecution {
                permission_behavior: PermissionBehavior::Deny,
                result: ToolResult::error(
                    &call,
                    json!({
                        "type": "error",
                        "code": "permission_denied",
                        "message": decision.message.unwrap_or_else(|| "tool access denied".to_owned()),
                        "permission_behavior": PermissionBehavior::Deny.as_str(),
                    }),
                ),
            }),
        }
    }

    fn load(&self, name: &str) -> ClawinResult<Box<dyn ToolHandler>> {
        let Some(index) = self.lookup.get(name) else {
            return Err(ClawinError::UnknownTool {
                name: name.to_owned(),
            });
        };

        Ok((self.tools[*index].load)())
    }
}

/// Build the Phase 3 built-in tool registry.
pub fn builtin_tool_registry() -> ToolRegistry {
    let mut registry = ToolRegistry::new();
    registry.register(
        ToolSpec {
            name: "file_read".to_owned(),
            description: "Read UTF-8 text files from the current project".to_owned(),
            input_schema_json: json!({
                "type": "object",
                "properties": {
                    "file_path": { "type": "string" },
                    "offset": { "type": "integer", "minimum": 1 },
                    "limit": { "type": "integer", "minimum": 1 }
                },
                "required": ["file_path"]
            }),
            kind: ToolKind::ReadOnly,
            source: ToolSource::Builtin,
        },
        || Box::new(FileReadTool),
    );
    registry
}

#[derive(Debug, Deserialize)]
struct FileReadInput {
    file_path: String,
    offset: Option<usize>,
    limit: Option<usize>,
}

struct FileReadTool;

impl ToolHandler for FileReadTool {
    fn validate(&self, input: &Value) -> ClawinResult<()> {
        let _: FileReadInput = serde_json::from_value(input.clone()).map_err(|error| {
            ClawinError::ToolInputInvalid {
                tool: "file_read".to_owned(),
                message: error.to_string(),
            }
        })?;
        Ok(())
    }

    fn check_permission(
        &self,
        runtime: &SessionRuntime,
        call: &ToolCall,
    ) -> ClawinResult<PermissionDecision> {
        let input: FileReadInput = serde_json::from_value(call.input.clone()).map_err(|error| {
            ClawinError::ToolInputInvalid {
                tool: call.tool_name.clone(),
                message: error.to_string(),
            }
        })?;
        let resolved = resolve_tool_path(runtime, &input.file_path);
        let project_root = runtime.project_root();

        if resolved.starts_with(project_root) {
            Ok(PermissionDecision::new(PermissionBehavior::Allow, None))
        } else {
            Ok(PermissionDecision::new(
                PermissionBehavior::Ask,
                Some("requested path is outside the project root".to_owned()),
            ))
        }
    }

    fn call(&self, runtime: &SessionRuntime, call: &ToolCall) -> ClawinResult<ToolResult> {
        let input: FileReadInput = serde_json::from_value(call.input.clone()).map_err(|error| {
            ClawinError::ToolInputInvalid {
                tool: call.tool_name.clone(),
                message: error.to_string(),
            }
        })?;

        let resolved = resolve_tool_path(runtime, &input.file_path);
        if is_unsupported_extension(&resolved) {
            return Ok(ToolResult::error(
                call,
                json!({
                    "type": "error",
                    "code": "unsupported_file_type",
                    "message": "file_read currently supports plain UTF-8 text files only",
                }),
            ));
        }

        let bytes = match fs::read(&resolved) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(ToolResult::error(
                    call,
                    json!({
                        "type": "error",
                        "code": "not_found",
                        "message": format!("file not found: {}", resolved.display()),
                    }),
                ));
            }
            Err(error) => {
                return Err(ClawinError::ToolExecution {
                    tool: call.tool_name.clone(),
                    message: error.to_string(),
                });
            }
        };

        if bytes.contains(&0) {
            return Ok(ToolResult::error(
                call,
                json!({
                    "type": "error",
                    "code": "unsupported_file_type",
                    "message": "file_read currently supports plain UTF-8 text files only",
                }),
            ));
        }

        let contents = match String::from_utf8(bytes) {
            Ok(contents) => contents,
            Err(_) => {
                return Ok(ToolResult::error(
                    call,
                    json!({
                        "type": "error",
                        "code": "unsupported_file_type",
                        "message": "file_read currently supports plain UTF-8 text files only",
                    }),
                ));
            }
        };

        let all_lines = contents.lines().collect::<Vec<_>>();
        let start_line = input.offset.unwrap_or(1).max(1);
        let zero_based_start = start_line.saturating_sub(1);
        let lines = all_lines
            .into_iter()
            .skip(zero_based_start)
            .take(input.limit.unwrap_or(usize::MAX))
            .collect::<Vec<_>>();
        let lines_read = lines.len();
        let end_line = if lines_read == 0 {
            start_line.saturating_sub(1)
        } else {
            start_line + lines_read - 1
        };

        Ok(ToolResult::success(
            call,
            json!({
                "type": "text",
                "start_line": start_line,
                "end_line": end_line,
                "content": lines.join("\n"),
            }),
        ))
    }
}

fn resolve_tool_path(runtime: &SessionRuntime, file_path: &str) -> PathBuf {
    let path = PathBuf::from(file_path);
    if path.is_absolute() {
        path
    } else {
        runtime.original_cwd().join(path)
    }
}

fn is_unsupported_extension(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|extension| extension.to_str()),
        Some("pdf" | "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp")
    )
}
