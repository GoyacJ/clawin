#![forbid(unsafe_code)]

//! Minimal tool registry, permission routing, and Phase 6A MCP extensions.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use clawin_core::PermissionResolver;
use clawin_core::{
    ClawinError, ClawinResult, PermissionBehavior, PermissionDecision, SessionRuntime, ToolCall,
    ToolKind, ToolResult, ToolSource, ToolSpec, WorktreeExitAction,
};
use clawin_integrations::{McpManager, McpResource};
use serde::Deserialize;
use serde_json::{Value, json};

type ToolLoader = Arc<dyn Fn() -> Box<dyn ToolHandler> + Send + Sync>;

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

/// Dynamic tool source used for MCP-backed specs and execution.
pub trait ToolCatalog: Send + Sync {
    fn tool_spec(&self, name: &str) -> Option<ToolSpec>;
    fn tool_specs(&self) -> Vec<ToolSpec>;
    fn execute(
        &self,
        call: ToolCall,
        runtime: &SessionRuntime,
    ) -> ClawinResult<Option<ToolExecution>>;
}

#[derive(Clone)]
struct RegisteredTool {
    spec: ToolSpec,
    load: ToolLoader,
}

/// Tool registry with stable built-ins plus dynamic catalogs.
#[derive(Clone, Default)]
pub struct ToolRegistry {
    tools: Vec<RegisteredTool>,
    lookup: BTreeMap<String, usize>,
    catalogs: Vec<Arc<dyn ToolCatalog>>,
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
    pub fn register<L>(&mut self, spec: ToolSpec, load: L)
    where
        L: Fn() -> Box<dyn ToolHandler> + Send + Sync + 'static,
    {
        let index = self.tools.len();
        self.lookup.insert(spec.name.clone(), index);
        self.tools.push(RegisteredTool {
            spec,
            load: Arc::new(load),
        });
    }

    /// Register a dynamic tool catalog.
    pub fn register_catalog(&mut self, catalog: Arc<dyn ToolCatalog>) {
        self.catalogs.push(catalog);
    }

    /// Read one tool spec by name from either the static registry or dynamic catalogs.
    pub fn spec(&self, name: &str) -> Option<ToolSpec> {
        self.lookup
            .get(name)
            .and_then(|index| self.tools.get(*index))
            .map(|entry| entry.spec.clone())
            .or_else(|| {
                self.catalogs
                    .iter()
                    .find_map(|catalog| catalog.tool_spec(name))
            })
    }

    /// Iterate over all available tool specs, including dynamic MCP tools.
    pub fn tool_specs(&self) -> impl Iterator<Item = ToolSpec> {
        let mut specs = self
            .tools
            .iter()
            .map(|entry| entry.spec.clone())
            .collect::<Vec<_>>();
        for catalog in &self.catalogs {
            specs.extend(catalog.tool_specs());
        }
        specs.into_iter()
    }

    /// Execute a tool call, returning a structured result and permission behavior.
    pub fn execute(&self, call: ToolCall, runtime: &SessionRuntime) -> ClawinResult<ToolExecution> {
        if self.lookup.contains_key(&call.tool_name) {
            return self.execute_static(call, runtime);
        }

        for catalog in &self.catalogs {
            if let Some(execution) = catalog.execute(call.clone(), runtime)? {
                return Ok(execution);
            }
        }

        Err(ClawinError::UnknownTool {
            name: call.tool_name,
        })
    }

    /// Execute a tool call while delegating `ask` permissions to an injected async resolver.
    pub async fn execute_with_resolver(
        &self,
        call: ToolCall,
        runtime: &SessionRuntime,
        resolver: &dyn PermissionResolver,
    ) -> ClawinResult<ToolExecution> {
        if self.lookup.contains_key(&call.tool_name) {
            return self
                .execute_static_with_resolver(call, runtime, resolver)
                .await;
        }

        for catalog in &self.catalogs {
            if let Some(execution) = catalog.execute(call.clone(), runtime)? {
                return Ok(execution);
            }
        }

        Err(ClawinError::UnknownTool {
            name: call.tool_name,
        })
    }

    fn execute_static(
        &self,
        call: ToolCall,
        runtime: &SessionRuntime,
    ) -> ClawinResult<ToolExecution> {
        let handler = self.load(&call.tool_name)?;
        handler.validate(&call.input)?;
        let mut decision = handler.check_permission(runtime, &call)?;
        if decision.behavior == PermissionBehavior::Ask {
            decision.message = None;
        }
        Self::execute_from_decision(handler.as_ref(), call, runtime, decision)
    }

    async fn execute_static_with_resolver(
        &self,
        call: ToolCall,
        runtime: &SessionRuntime,
        resolver: &dyn PermissionResolver,
    ) -> ClawinResult<ToolExecution> {
        let handler = self.load(&call.tool_name)?;
        handler.validate(&call.input)?;
        let decision = handler.check_permission(runtime, &call)?;
        let resolved = match decision.behavior {
            PermissionBehavior::Ask => resolver.resolve(&call, decision).await?,
            _ => decision,
        };
        Self::execute_from_decision(handler.as_ref(), call, runtime, resolved)
    }

    fn load(&self, name: &str) -> ClawinResult<Box<dyn ToolHandler>> {
        let Some(index) = self.lookup.get(name) else {
            return Err(ClawinError::UnknownTool {
                name: name.to_owned(),
            });
        };

        Ok((self.tools[*index].load)())
    }

    fn execute_from_decision(
        handler: &dyn ToolHandler,
        call: ToolCall,
        runtime: &SessionRuntime,
        decision: PermissionDecision,
    ) -> ClawinResult<ToolExecution> {
        match decision.behavior {
            PermissionBehavior::Allow => Ok(ToolExecution {
                permission_behavior: PermissionBehavior::Allow,
                result: handler.call(runtime, &call)?,
            }),
            PermissionBehavior::Ask => Ok(ToolExecution {
                permission_behavior: PermissionBehavior::Ask,
                result: permission_denied_result(
                    &call,
                    PermissionBehavior::Ask,
                    Some(decision.message.unwrap_or_else(|| {
                        "permission prompt not implemented yet for paths outside the project root"
                            .to_owned()
                    })),
                ),
            }),
            PermissionBehavior::Deny => Ok(ToolExecution {
                permission_behavior: PermissionBehavior::Deny,
                result: permission_denied_result(&call, PermissionBehavior::Deny, decision.message),
            }),
        }
    }
}

/// Build the baseline built-in tool registry.
pub fn builtin_tool_registry() -> ToolRegistry {
    let mut registry = ToolRegistry::new();
    register_file_read_tool(&mut registry);
    register_enter_worktree_tool(&mut registry);
    register_exit_worktree_tool(&mut registry);
    registry
}

/// Build the built-in tool registry with Phase 6A MCP support.
pub fn builtin_tool_registry_with_mcp(manager: Arc<McpManager>) -> ToolRegistry {
    let mut registry = builtin_tool_registry();
    register_list_mcp_resources_tool(&mut registry, Arc::clone(&manager));
    register_read_mcp_resource_tool(&mut registry, Arc::clone(&manager));
    registry.register_catalog(Arc::new(McpToolCatalog::new(manager)));
    registry
}

fn register_file_read_tool(registry: &mut ToolRegistry) {
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
}

fn register_enter_worktree_tool(registry: &mut ToolRegistry) {
    registry.register(
        ToolSpec {
            name: "EnterWorktree".to_owned(),
            description: "Create and enter a session-owned git worktree".to_owned(),
            input_schema_json: json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string" }
                }
            }),
            kind: ToolKind::ReadOnly,
            source: ToolSource::Builtin,
        },
        || Box::new(EnterWorktreeTool),
    );
}

fn register_exit_worktree_tool(registry: &mut ToolRegistry) {
    registry.register(
        ToolSpec {
            name: "ExitWorktree".to_owned(),
            description: "Leave or remove the active session-owned git worktree".to_owned(),
            input_schema_json: json!({
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["keep", "remove"] },
                    "discard_changes": { "type": "boolean" }
                },
                "required": ["action"]
            }),
            kind: ToolKind::ReadOnly,
            source: ToolSource::Builtin,
        },
        || Box::new(ExitWorktreeTool),
    );
}

fn register_list_mcp_resources_tool(registry: &mut ToolRegistry, manager: Arc<McpManager>) {
    registry.register(
        ToolSpec {
            name: "list_mcp_resources".to_owned(),
            description: "List cached MCP resources for all connected servers or one named server"
                .to_owned(),
            input_schema_json: json!({
                "type": "object",
                "properties": {
                    "server": { "type": "string" }
                }
            }),
            kind: ToolKind::ReadOnly,
            source: ToolSource::Builtin,
        },
        move || {
            Box::new(ListMcpResourcesTool {
                manager: Arc::clone(&manager),
            })
        },
    );
}

fn register_read_mcp_resource_tool(registry: &mut ToolRegistry, manager: Arc<McpManager>) {
    registry.register(
        ToolSpec {
            name: "read_mcp_resource".to_owned(),
            description: "Read one text MCP resource from a connected server".to_owned(),
            input_schema_json: json!({
                "type": "object",
                "properties": {
                    "server": { "type": "string" },
                    "uri": { "type": "string" }
                },
                "required": ["server", "uri"]
            }),
            kind: ToolKind::ReadOnly,
            source: ToolSource::Builtin,
        },
        move || {
            Box::new(ReadMcpResourceTool {
                manager: Arc::clone(&manager),
            })
        },
    );
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
        let _: FileReadInput = parse_tool_input("file_read", input)?;
        Ok(())
    }

    fn check_permission(
        &self,
        runtime: &SessionRuntime,
        call: &ToolCall,
    ) -> ClawinResult<PermissionDecision> {
        let input: FileReadInput = parse_call_input(call)?;
        let resolved = resolve_tool_path(runtime, &input.file_path);
        let project_root = runtime.active_project_root();

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
        let input: FileReadInput = parse_call_input(call)?;

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

#[derive(Debug, Deserialize)]
struct EnterWorktreeInput {
    name: Option<String>,
}

struct EnterWorktreeTool;

impl ToolHandler for EnterWorktreeTool {
    fn validate(&self, input: &Value) -> ClawinResult<()> {
        let _: EnterWorktreeInput = parse_tool_input("EnterWorktree", input)?;
        Ok(())
    }

    fn check_permission(
        &self,
        _runtime: &SessionRuntime,
        _call: &ToolCall,
    ) -> ClawinResult<PermissionDecision> {
        Ok(PermissionDecision::new(PermissionBehavior::Allow, None))
    }

    fn call(&self, runtime: &SessionRuntime, call: &ToolCall) -> ClawinResult<ToolResult> {
        let input: EnterWorktreeInput = parse_call_input(call)?;
        let Some(manager) = runtime.worktree_manager() else {
            return Err(ClawinError::InvalidConfiguration {
                message: "worktree manager is unavailable".to_owned(),
            });
        };

        let worktree = manager.enter_worktree(runtime, input.name.as_deref())?;
        if let Some(store) = runtime.session_store() {
            store.save_worktree_state(runtime, Some(&worktree))?;
        }

        Ok(ToolResult::success(
            call,
            json!({
                "type": "worktree",
                "action": "entered",
                "path": worktree.worktree_path,
                "branch": worktree.branch,
            }),
        ))
    }
}

#[derive(Debug, Deserialize)]
struct ExitWorktreeInput {
    action: WorktreeExitAction,
    #[serde(default)]
    discard_changes: bool,
}

struct ExitWorktreeTool;

impl ToolHandler for ExitWorktreeTool {
    fn validate(&self, input: &Value) -> ClawinResult<()> {
        let _: ExitWorktreeInput = parse_tool_input("ExitWorktree", input)?;
        Ok(())
    }

    fn check_permission(
        &self,
        _runtime: &SessionRuntime,
        _call: &ToolCall,
    ) -> ClawinResult<PermissionDecision> {
        Ok(PermissionDecision::new(PermissionBehavior::Allow, None))
    }

    fn call(&self, runtime: &SessionRuntime, call: &ToolCall) -> ClawinResult<ToolResult> {
        let input: ExitWorktreeInput = parse_call_input(call)?;
        let Some(manager) = runtime.worktree_manager() else {
            return Err(ClawinError::InvalidConfiguration {
                message: "worktree manager is unavailable".to_owned(),
            });
        };

        let previous = manager.exit_worktree(runtime, input.action, input.discard_changes)?;
        if let Some(store) = runtime.session_store() {
            store.save_worktree_state(runtime, runtime.active_worktree().as_ref())?;
        }

        Ok(ToolResult::success(
            call,
            json!({
                "type": "worktree",
                "action": match input.action {
                    WorktreeExitAction::Keep => "kept",
                    WorktreeExitAction::Remove => "removed",
                },
                "previous_path": previous.as_ref().map(|worktree| worktree.worktree_path.clone()),
            }),
        ))
    }
}

#[derive(Debug, Deserialize)]
struct ListMcpResourcesInput {
    server: Option<String>,
}

struct ListMcpResourcesTool {
    manager: Arc<McpManager>,
}

impl ToolHandler for ListMcpResourcesTool {
    fn validate(&self, input: &Value) -> ClawinResult<()> {
        let _: ListMcpResourcesInput = parse_tool_input("list_mcp_resources", input)?;
        Ok(())
    }

    fn check_permission(
        &self,
        _runtime: &SessionRuntime,
        _call: &ToolCall,
    ) -> ClawinResult<PermissionDecision> {
        Ok(PermissionDecision::new(PermissionBehavior::Allow, None))
    }

    fn call(&self, _runtime: &SessionRuntime, call: &ToolCall) -> ClawinResult<ToolResult> {
        let input: ListMcpResourcesInput = parse_call_input(call)?;
        match self.manager.list_resources(input.server.as_deref()) {
            Ok(resources) => Ok(ToolResult::success(
                call,
                json!({
                    "resources": resources
                        .into_iter()
                        .map(resource_to_json)
                        .collect::<Vec<_>>(),
                }),
            )),
            Err(error) => Ok(ToolResult::error(
                call,
                json!({
                    "type": "error",
                    "code": "mcp_resource_list_failed",
                    "message": error.to_string(),
                }),
            )),
        }
    }
}

#[derive(Debug, Deserialize)]
struct ReadMcpResourceInput {
    server: String,
    uri: String,
}

struct ReadMcpResourceTool {
    manager: Arc<McpManager>,
}

impl ToolHandler for ReadMcpResourceTool {
    fn validate(&self, input: &Value) -> ClawinResult<()> {
        let _: ReadMcpResourceInput = parse_tool_input("read_mcp_resource", input)?;
        Ok(())
    }

    fn check_permission(
        &self,
        _runtime: &SessionRuntime,
        _call: &ToolCall,
    ) -> ClawinResult<PermissionDecision> {
        Ok(PermissionDecision::new(PermissionBehavior::Allow, None))
    }

    fn call(&self, _runtime: &SessionRuntime, call: &ToolCall) -> ClawinResult<ToolResult> {
        let input: ReadMcpResourceInput = parse_call_input(call)?;
        let result = match self.manager.read_resource(&input.server, &input.uri) {
            Ok(result) => result,
            Err(error) => {
                return Ok(ToolResult::error(
                    call,
                    json!({
                        "type": "error",
                        "code": "mcp_resource_read_failed",
                        "message": error.to_string(),
                    }),
                ));
            }
        };

        let Some(contents) = result.get("contents").and_then(Value::as_array) else {
            return Ok(ToolResult::error(
                call,
                json!({
                    "type": "error",
                    "code": "invalid_resource_response",
                    "message": "MCP resource read result did not contain a contents array",
                }),
            ));
        };

        let mut rendered = Vec::with_capacity(contents.len());
        for content in contents {
            if content.get("text").and_then(Value::as_str).is_none() {
                return Ok(ToolResult::error(
                    call,
                    json!({
                        "type": "error",
                        "code": "unsupported_binary_resource",
                        "message": "read_mcp_resource currently supports text resources only",
                    }),
                ));
            }

            rendered.push(json!({
                "uri": content.get("uri").cloned().unwrap_or_else(|| Value::String(input.uri.clone())),
                "mimeType": content.get("mimeType").cloned().unwrap_or(Value::Null),
                "text": content.get("text").cloned().unwrap_or(Value::Null),
            }));
        }

        Ok(ToolResult::success(
            call,
            json!({
                "server": input.server,
                "uri": input.uri,
                "contents": rendered,
            }),
        ))
    }
}

struct McpToolCatalog {
    manager: Arc<McpManager>,
}

impl McpToolCatalog {
    fn new(manager: Arc<McpManager>) -> Self {
        Self { manager }
    }
}

impl ToolCatalog for McpToolCatalog {
    fn tool_spec(&self, name: &str) -> Option<ToolSpec> {
        self.manager.tool_spec(name)
    }

    fn tool_specs(&self) -> Vec<ToolSpec> {
        self.manager.tool_specs()
    }

    fn execute(
        &self,
        call: ToolCall,
        _runtime: &SessionRuntime,
    ) -> ClawinResult<Option<ToolExecution>> {
        if self.manager.tool_spec(&call.tool_name).is_none() {
            return Ok(None);
        }

        let result = match self.manager.call_tool(&call) {
            Ok(result) => result,
            Err(error) => ToolResult::error(
                &call,
                json!({
                    "type": "error",
                    "code": "mcp_tool_call_failed",
                    "message": error.to_string(),
                }),
            ),
        };

        Ok(Some(ToolExecution {
            permission_behavior: PermissionBehavior::Allow,
            result,
        }))
    }
}

fn parse_tool_input<T>(tool: &str, input: &Value) -> ClawinResult<T>
where
    T: for<'de> Deserialize<'de>,
{
    serde_json::from_value(input.clone()).map_err(|error| ClawinError::ToolInputInvalid {
        tool: tool.to_owned(),
        message: error.to_string(),
    })
}

fn parse_call_input<T>(call: &ToolCall) -> ClawinResult<T>
where
    T: for<'de> Deserialize<'de>,
{
    parse_tool_input(&call.tool_name, &call.input)
}

fn resource_to_json(resource: McpResource) -> Value {
    json!({
        "server": resource.server,
        "uri": resource.uri,
        "name": resource.name,
        "mimeType": resource.mime_type,
        "description": resource.description,
    })
}

fn resolve_tool_path(runtime: &SessionRuntime, file_path: &str) -> PathBuf {
    let path = PathBuf::from(file_path);
    let resolved = if path.is_absolute() {
        path
    } else {
        runtime.current_cwd().join(path)
    };

    normalize_tool_path(&resolved)
}

fn normalize_tool_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    let mut has_root = false;
    let mut normal_depth = 0usize;

    for component in path.components() {
        match component {
            Component::Prefix(value) => normalized.push(value.as_os_str()),
            Component::RootDir => {
                normalized.push(Path::new(std::path::MAIN_SEPARATOR_STR));
                has_root = true;
                normal_depth = 0;
            }
            Component::CurDir => {}
            Component::ParentDir => {
                if normal_depth > 0 {
                    normalized.pop();
                    normal_depth -= 1;
                } else if !has_root {
                    normalized.push(component.as_os_str());
                }
            }
            Component::Normal(value) => {
                normalized.push(value);
                normal_depth += 1;
            }
        }
    }

    if normalized.as_os_str().is_empty() {
        if has_root {
            PathBuf::from(std::path::MAIN_SEPARATOR_STR)
        } else {
            PathBuf::from(".")
        }
    } else {
        normalized
    }
}

fn is_unsupported_extension(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|extension| extension.to_str()),
        Some("pdf" | "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp")
    )
}

fn permission_denied_result(
    call: &ToolCall,
    behavior: PermissionBehavior,
    message: Option<String>,
) -> ToolResult {
    ToolResult::error(
        call,
        json!({
            "type": "error",
            "code": "permission_denied",
            "message": message.unwrap_or_else(|| "tool access denied".to_owned()),
            "permission_behavior": behavior.as_str(),
        }),
    )
}
