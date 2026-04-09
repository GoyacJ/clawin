#![forbid(unsafe_code)]

//! MCP, skills, and plugin runtime integration support for the Phase 6 baseline.

mod bridge;
mod plugins;
mod skills;

use std::collections::BTreeMap;
use std::io::{BufReader, Read};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use clawin_config::LoadedConfigSnapshot;
use clawin_core::{
    ClawinError, ClawinResult, ToolCall, ToolKind, ToolResult, ToolSource, ToolSpec,
};
use clawin_platform::{ProcessSpawnRequest, ProcessSpawner, SpawnedProcess};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use tracing::debug;

const DEFAULT_PROTOCOL_VERSION: &str = "2025-03-26";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

pub use bridge::{
    BRIDGE_POINTER_FILE_NAME, BRIDGE_POINTER_TTL, BridgeConnectRequest, BridgeManager,
    BridgePointerStore, BridgeTransportConnector, BridgeTransportPoll, BridgeTransportSession,
    FakeBridgeConnector, FakeBridgeRemote, ReconnectPolicy, UnavailableBridgeConnector,
};
pub use plugins::{
    LoadedPlugin, LoadedPluginCommand, LoadedPluginsSnapshot, PLUGINS_DIRECTORY_NAME,
    PluginMcpServerEntry, PluginRuntimeSource, PluginRuntimeStatus, load_plugins_snapshot,
};
pub use skills::{
    LoadedSkill, LoadedSkillsSnapshot, SKILLS_DIRECTORY_NAME, SkillLoadError, SkillSource,
    load_skills_snapshot,
};

/// Normalize a server or tool name into the upstream MCP-safe token format.
pub fn normalize_name_for_mcp(name: &str) -> String {
    name.chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-') {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

fn build_mcp_tool_name(server_name: &str, tool_name: &str) -> String {
    format!(
        "mcp__{}__{}",
        normalize_name_for_mcp(server_name),
        normalize_name_for_mcp(tool_name)
    )
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub enum McpConfigScope {
    User,
    Project,
}

impl McpConfigScope {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Project => "project",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub enum McpServerStatus {
    Connected,
    Failed,
}

impl McpServerStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Connected => "connected",
            Self::Failed => "failed",
        }
    }
}

/// Connected/failed MCP server snapshot exposed to commands, tools, and bootstrap.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct McpServerSnapshot {
    pub name: String,
    pub scope: McpConfigScope,
    pub transport: String,
    pub status: McpServerStatus,
    pub tool_count: usize,
    pub resource_count: usize,
    pub last_error: Option<String>,
}

impl McpServerSnapshot {
    pub fn scope_label(&self) -> &'static str {
        self.scope.as_str()
    }

    pub fn status_label(&self) -> &'static str {
        self.status.as_str()
    }
}

/// MCP resource metadata cached from `resources/list`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct McpResource {
    pub uri: String,
    pub name: String,
    pub mime_type: Option<String>,
    pub description: Option<String>,
    pub server: String,
}

#[derive(Clone, Debug)]
struct DiscoveredTool {
    spec: ToolSpec,
    original_name: String,
}

#[derive(Clone, Debug)]
struct ValidatedServerConfig {
    scope: McpConfigScope,
    command: String,
    args: Vec<String>,
    env: BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RawServerEntry {
    scope: McpConfigScope,
    value: Value,
}

struct LiveConnection {
    process: Box<dyn SpawnedProcess>,
    responses: Receiver<Result<Value, String>>,
    next_request_id: u64,
}

struct ServerState {
    snapshot: McpServerSnapshot,
    tools: Vec<DiscoveredTool>,
    resources: Vec<McpResource>,
    live: Option<LiveConnection>,
}

impl ServerState {
    fn disconnected(snapshot: McpServerSnapshot) -> Self {
        Self {
            snapshot,
            tools: Vec::new(),
            resources: Vec::new(),
            live: None,
        }
    }
}

struct ManagerState {
    merged_entries: BTreeMap<String, RawServerEntry>,
    plugin_entries: BTreeMap<String, RawServerEntry>,
    servers: BTreeMap<String, ServerState>,
}

/// Shared stdio MCP manager for the Phase 6A baseline.
#[derive(Clone)]
pub struct McpManager {
    spawner: Arc<dyn ProcessSpawner>,
    state: Arc<Mutex<ManagerState>>,
}

impl std::fmt::Debug for McpManager {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("McpManager")
            .field("configured_servers", &self.has_configured_servers())
            .field("server_count", &self.server_snapshots().len())
            .finish()
    }
}

impl McpManager {
    /// Build the stdio MCP manager from already-loaded settings documents.
    pub fn from_loaded_config(
        snapshot: &LoadedConfigSnapshot,
        spawner: Arc<dyn ProcessSpawner>,
    ) -> ClawinResult<Self> {
        let merged_entries = merge_server_entries(snapshot)?;
        let manager = Self {
            spawner,
            state: Arc::new(Mutex::new(ManagerState {
                merged_entries,
                plugin_entries: BTreeMap::new(),
                servers: BTreeMap::new(),
            })),
        };
        manager.reload()?;
        Ok(manager)
    }

    /// Whether the current config snapshot declared any MCP servers at all.
    pub fn has_configured_servers(&self) -> bool {
        let state = self
            .state
            .lock()
            .expect("mcp manager state lock should be available");
        !(state.merged_entries.is_empty() && state.plugin_entries.is_empty())
    }

    /// Read the current server snapshots.
    pub fn server_snapshots(&self) -> Vec<McpServerSnapshot> {
        self.state
            .lock()
            .expect("mcp manager state lock should be available")
            .servers
            .values()
            .map(|state| state.snapshot.clone())
            .collect()
    }

    /// Read the current MCP tool specs with fully-qualified `mcp__server__tool` names.
    pub fn tool_specs(&self) -> Vec<ToolSpec> {
        self.state
            .lock()
            .expect("mcp manager state lock should be available")
            .servers
            .values()
            .flat_map(|state| state.tools.iter().map(|tool| tool.spec.clone()))
            .collect()
    }

    /// Find a single MCP tool spec by fully-qualified name.
    pub fn tool_spec(&self, name: &str) -> Option<ToolSpec> {
        self.tool_specs().into_iter().find(|spec| spec.name == name)
    }

    /// Reload every configured MCP server from the original merged config snapshot.
    pub fn reload(&self) -> ClawinResult<()> {
        let merged_entries = self.combined_entries();

        let mut next_servers = BTreeMap::new();
        for (name, entry) in merged_entries {
            let server_state = match validate_server_config(&entry) {
                Ok(config) => match connect_server(&name, &config, self.spawner.as_ref()) {
                    Ok(state) => state,
                    Err(error) => ServerState::disconnected(McpServerSnapshot {
                        name: name.clone(),
                        scope: config.scope,
                        transport: "stdio".to_owned(),
                        status: McpServerStatus::Failed,
                        tool_count: 0,
                        resource_count: 0,
                        last_error: Some(error_message(&error)),
                    }),
                },
                Err(message) => ServerState::disconnected(McpServerSnapshot {
                    name: name.clone(),
                    scope: entry.scope,
                    transport: transport_label(&entry.value),
                    status: McpServerStatus::Failed,
                    tool_count: 0,
                    resource_count: 0,
                    last_error: Some(message),
                }),
            };

            next_servers.insert(name, server_state);
        }

        let mut state = self
            .state
            .lock()
            .expect("mcp manager state lock should be available");
        disconnect_servers(&mut state.servers);
        state.servers = next_servers;
        Ok(())
    }

    /// Merge plugin-contributed server declarations into the managed MCP entry set.
    pub fn set_plugin_servers(&self, plugins: &LoadedPluginsSnapshot) -> ClawinResult<()> {
        let plugin_entries = plugins
            .mcp_server_entries()
            .into_iter()
            .map(|entry| {
                (
                    entry.name().to_owned(),
                    RawServerEntry {
                        scope: entry.scope(),
                        value: entry.value().clone(),
                    },
                )
            })
            .collect::<BTreeMap<_, _>>();

        let mut state = self
            .state
            .lock()
            .expect("mcp manager state lock should be available");
        if state.plugin_entries == plugin_entries {
            return Ok(());
        }
        state.plugin_entries = plugin_entries;
        drop(state);

        self.reload()
    }

    /// Render a stable text summary used by `/mcp list` and `/mcp reload`.
    pub fn render_status_report(&self, heading: &str) -> String {
        let snapshots = self.server_snapshots();
        if snapshots.is_empty() {
            return format!("{heading}\n(no MCP servers configured)\n");
        }

        let mut lines = vec![heading.to_owned()];
        for server in snapshots {
            let mut line = format!(
                "- {} scope={} transport={} status={} tools={} resources={}",
                server.name,
                server.scope_label(),
                server.transport,
                server.status_label(),
                server.tool_count,
                server.resource_count
            );
            if let Some(error) = server.last_error {
                line.push_str(" error=");
                line.push_str(&error);
            }
            lines.push(line);
        }
        lines.push(String::new());
        lines.join("\n")
    }

    /// Return the current cached MCP resources, optionally filtered to one server.
    pub fn list_resources(&self, server: Option<&str>) -> ClawinResult<Vec<McpResource>> {
        let state = self
            .state
            .lock()
            .expect("mcp manager state lock should be available");

        if let Some(server) = server {
            let Some(server_state) = state.servers.get(server) else {
                return Err(ClawinError::InvalidConfiguration {
                    message: format!("MCP server `{server}` not found"),
                });
            };
            if server_state.snapshot.status != McpServerStatus::Connected {
                return Err(ClawinError::InvalidConfiguration {
                    message: format!("MCP server `{server}` is not connected"),
                });
            }
            return Ok(server_state.resources.clone());
        }

        Ok(state
            .servers
            .values()
            .filter(|server_state| server_state.snapshot.status == McpServerStatus::Connected)
            .flat_map(|server_state| server_state.resources.clone())
            .collect())
    }

    /// Read one MCP resource from a connected server.
    pub fn read_resource(&self, server: &str, uri: &str) -> ClawinResult<Value> {
        let mut state = self
            .state
            .lock()
            .expect("mcp manager state lock should be available");
        let Some(server_state) = state.servers.get_mut(server) else {
            return Err(ClawinError::InvalidConfiguration {
                message: format!("MCP server `{server}` not found"),
            });
        };
        let Some(live) = server_state.live.as_mut() else {
            return Err(ClawinError::InvalidConfiguration {
                message: format!("MCP server `{server}` is not connected"),
            });
        };

        request(
            live,
            "resources/read",
            json!({
                "uri": uri,
            }),
        )
    }

    /// Execute one fully-qualified MCP tool call against its owning server.
    pub fn call_tool(&self, call: &ToolCall) -> ClawinResult<ToolResult> {
        let mut state = self
            .state
            .lock()
            .expect("mcp manager state lock should be available");

        let Some((server_name, original_name)) =
            state.servers.iter().find_map(|(server_name, state)| {
                state
                    .tools
                    .iter()
                    .find(|tool| tool.spec.name == call.tool_name)
                    .map(|tool| (server_name.clone(), tool.original_name.clone()))
            })
        else {
            return Err(ClawinError::UnknownTool {
                name: call.tool_name.clone(),
            });
        };

        let server_state = state
            .servers
            .get_mut(&server_name)
            .expect("server should still exist");
        let Some(live) = server_state.live.as_mut() else {
            return Err(ClawinError::InvalidConfiguration {
                message: format!("MCP server `{server_name}` is not connected"),
            });
        };

        let result = request(
            live,
            "tools/call",
            json!({
                "name": original_name,
                "arguments": call.input.clone(),
            }),
        )?;
        let is_error = result
            .get("isError")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        Ok(ToolResult {
            call_id: call.call_id.clone(),
            tool_name: call.tool_name.clone(),
            is_error,
            content: result,
        })
    }

    fn combined_entries(&self) -> BTreeMap<String, RawServerEntry> {
        let state = self
            .state
            .lock()
            .expect("mcp manager state lock should be available");
        let mut merged = state.merged_entries.clone();
        merged.extend(state.plugin_entries.clone());
        merged
    }
}

impl Drop for McpManager {
    fn drop(&mut self) {
        if let Ok(mut state) = self.state.lock() {
            disconnect_servers(&mut state.servers);
        }
    }
}

fn merge_server_entries(
    snapshot: &LoadedConfigSnapshot,
) -> ClawinResult<BTreeMap<String, RawServerEntry>> {
    let mut merged = BTreeMap::new();

    if let Some(settings) = snapshot.global_settings() {
        let entries = extract_mcp_servers(settings.extra.get("mcpServers"))?;
        for (name, value) in entries {
            merged.insert(
                name,
                RawServerEntry {
                    scope: McpConfigScope::User,
                    value,
                },
            );
        }
    }

    if let Some(settings) = snapshot.project_settings() {
        let entries = extract_mcp_servers(settings.extra.get("mcpServers"))?;
        for (name, value) in entries {
            merged.insert(
                name,
                RawServerEntry {
                    scope: McpConfigScope::Project,
                    value,
                },
            );
        }
    }

    Ok(merged)
}

fn extract_mcp_servers(value: Option<&Value>) -> ClawinResult<BTreeMap<String, Value>> {
    let Some(value) = value else {
        return Ok(BTreeMap::new());
    };
    let Some(object) = value.as_object() else {
        return Err(ClawinError::InvalidConfiguration {
            message: "mcpServers must be a JSON object".to_owned(),
        });
    };

    Ok(object
        .iter()
        .map(|(name, value)| (name.clone(), value.clone()))
        .collect())
}

fn validate_server_config(entry: &RawServerEntry) -> Result<ValidatedServerConfig, String> {
    let Some(object) = entry.value.as_object() else {
        return Err("server config must be a JSON object".to_owned());
    };
    let transport = object
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("stdio");
    if transport != "stdio" {
        return Err(format!(
            "only stdio MCP servers are supported in Phase 6A, got `{transport}`"
        ));
    }

    let command = object
        .get("command")
        .and_then(Value::as_str)
        .filter(|command| !command.trim().is_empty())
        .ok_or_else(|| "stdio server requires a non-empty `command`".to_owned())?;

    let args = match object.get("args") {
        None => Vec::new(),
        Some(Value::Array(values)) => values
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .map(expand_env_vars)
                    .ok_or_else(|| "`args` must contain strings only".to_owned())
            })
            .collect::<Result<Vec<_>, _>>()?,
        Some(_) => return Err("`args` must be an array of strings".to_owned()),
    };

    let env = match object.get("env") {
        None => BTreeMap::new(),
        Some(Value::Object(entries)) => entries
            .iter()
            .map(|(key, value)| {
                value
                    .as_str()
                    .map(|value| (key.clone(), expand_env_vars(value)))
                    .ok_or_else(|| "`env` must map strings to strings".to_owned())
            })
            .collect::<Result<BTreeMap<_, _>, _>>()?,
        Some(_) => return Err("`env` must be an object of string values".to_owned()),
    };

    Ok(ValidatedServerConfig {
        scope: entry.scope,
        command: expand_env_vars(command),
        args,
        env,
    })
}

fn connect_server(
    name: &str,
    config: &ValidatedServerConfig,
    spawner: &dyn ProcessSpawner,
) -> ClawinResult<ServerState> {
    let mut process = spawner
        .spawn(&ProcessSpawnRequest {
            command: config.command.clone(),
            args: config.args.clone(),
            env: config.env.clone(),
        })
        .map_err(|error| ClawinError::InvalidConfiguration {
            message: format!("failed to spawn MCP server `{name}`: {error}"),
        })?;
    let stdout = process
        .take_stdout()
        .map_err(|error| ClawinError::InvalidConfiguration {
            message: format!("failed to take stdout for MCP server `{name}`: {error}"),
        })?;
    let responses = spawn_reader_thread(name.to_owned(), stdout);
    let mut live = LiveConnection {
        process,
        responses,
        next_request_id: 1,
    };

    let initialize = request(
        &mut live,
        "initialize",
        json!({
            "protocolVersion": DEFAULT_PROTOCOL_VERSION,
            "capabilities": {
                "tools": {},
                "resources": {}
            },
            "clientInfo": {
                "name": "clawin",
                "version": env!("CARGO_PKG_VERSION")
            }
        }),
    )?;
    notify_initialized(&mut live)?;

    let capabilities = initialize
        .get("capabilities")
        .cloned()
        .unwrap_or_else(|| Value::Object(Map::new()));
    let tool_values = if capabilities.get("tools").is_some() {
        request(&mut live, "tools/list", json!({}))?
            .get("tools")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let resource_values = if capabilities.get("resources").is_some() {
        request(&mut live, "resources/list", json!({}))?
            .get("resources")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    let tools = parse_discovered_tools(name, tool_values);
    let resources = parse_resources(name, resource_values);
    let snapshot = McpServerSnapshot {
        name: name.to_owned(),
        scope: config.scope,
        transport: "stdio".to_owned(),
        status: McpServerStatus::Connected,
        tool_count: tools.len(),
        resource_count: resources.len(),
        last_error: None,
    };

    debug!(
        server = name,
        tool_count = snapshot.tool_count,
        resource_count = snapshot.resource_count,
        "connected MCP server"
    );

    Ok(ServerState {
        snapshot,
        tools,
        resources,
        live: Some(live),
    })
}

fn parse_discovered_tools(server_name: &str, values: Vec<Value>) -> Vec<DiscoveredTool> {
    values
        .into_iter()
        .filter_map(|value| {
            let name = value.get("name")?.as_str()?.to_owned();
            let description = value
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            let input_schema_json = value
                .get("inputSchema")
                .cloned()
                .unwrap_or_else(|| json!({ "type": "object" }));

            Some(DiscoveredTool {
                spec: ToolSpec {
                    name: build_mcp_tool_name(server_name, &name),
                    description,
                    input_schema_json,
                    kind: ToolKind::Unknown,
                    source: ToolSource::Mcp,
                },
                original_name: name,
            })
        })
        .collect()
}

fn parse_resources(server_name: &str, values: Vec<Value>) -> Vec<McpResource> {
    values
        .into_iter()
        .filter_map(|value| {
            Some(McpResource {
                uri: value.get("uri")?.as_str()?.to_owned(),
                name: value.get("name")?.as_str()?.to_owned(),
                mime_type: value
                    .get("mimeType")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned),
                description: value
                    .get("description")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned),
                server: server_name.to_owned(),
            })
        })
        .collect()
}

fn spawn_reader_thread(
    server_name: String,
    stdout: Box<dyn Read + Send>,
) -> Receiver<Result<Value, String>> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        loop {
            match read_framed_message(&mut reader) {
                Ok(Some(message)) => {
                    if tx.send(Ok(message)).is_err() {
                        break;
                    }
                }
                Ok(None) => break,
                Err(error) => {
                    let _ = tx.send(Err(format!(
                        "failed to read MCP message from `{server_name}`: {error}"
                    )));
                    break;
                }
            }
        }
    });
    rx
}

fn read_framed_message(reader: &mut impl Read) -> std::io::Result<Option<Value>> {
    let mut header_bytes = Vec::new();
    let mut byte = [0_u8; 1];

    loop {
        let read = reader.read(&mut byte)?;
        if read == 0 {
            if header_bytes.is_empty() {
                return Ok(None);
            }
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "unexpected EOF while reading MCP headers",
            ));
        }
        header_bytes.push(byte[0]);
        if header_bytes.ends_with(b"\r\n\r\n") {
            break;
        }
    }

    let headers = String::from_utf8(header_bytes).map_err(|error| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("invalid UTF-8 in MCP headers: {error}"),
        )
    })?;
    let content_length = headers
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            if name.eq_ignore_ascii_case("Content-Length") {
                value.trim().parse::<usize>().ok()
            } else {
                None
            }
        })
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "missing Content-Length header",
            )
        })?;

    let mut payload = vec![0_u8; content_length];
    reader.read_exact(&mut payload)?;
    serde_json::from_slice(&payload).map(Some).map_err(|error| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("invalid MCP JSON payload: {error}"),
        )
    })
}

fn request(live: &mut LiveConnection, method: &str, params: Value) -> ClawinResult<Value> {
    let request_id = live.next_request_id;
    live.next_request_id = live.next_request_id.saturating_add(1);

    write_message(
        live.process.as_mut(),
        &json!({
            "jsonrpc": "2.0",
            "id": request_id,
            "method": method,
            "params": params,
        }),
    )?;

    loop {
        let response = match live.responses.recv_timeout(REQUEST_TIMEOUT) {
            Ok(response) => response,
            Err(RecvTimeoutError::Timeout) => {
                let _ = live.process.kill();
                return Err(ClawinError::InvalidConfiguration {
                    message: format!("timed out waiting for MCP response to `{method}`"),
                });
            }
            Err(RecvTimeoutError::Disconnected) => {
                return Err(ClawinError::InvalidConfiguration {
                    message: format!("MCP response channel disconnected during `{method}`"),
                });
            }
        };

        let response = response.map_err(|message| ClawinError::InvalidConfiguration { message })?;
        if response.get("id").and_then(Value::as_u64) != Some(request_id) {
            continue;
        }
        if let Some(error) = response.get("error") {
            return Err(ClawinError::InvalidConfiguration {
                message: format!("MCP request `{method}` failed: {error}"),
            });
        }
        return Ok(response.get("result").cloned().unwrap_or(Value::Null));
    }
}

fn notify_initialized(live: &mut LiveConnection) -> ClawinResult<()> {
    write_message(
        live.process.as_mut(),
        &json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized",
            "params": {}
        }),
    )
}

fn write_message(process: &mut dyn SpawnedProcess, value: &Value) -> ClawinResult<()> {
    let payload = serde_json::to_vec(value).map_err(|error| ClawinError::InvalidConfiguration {
        message: format!("failed to serialize MCP request: {error}"),
    })?;
    let mut framed = format!("Content-Length: {}\r\n\r\n", payload.len()).into_bytes();
    framed.extend(payload);

    process
        .write_stdin(&framed)
        .map_err(|error| ClawinError::InvalidConfiguration {
            message: format!("failed to write MCP request: {error}"),
        })?;
    process
        .flush_stdin()
        .map_err(|error| ClawinError::InvalidConfiguration {
            message: format!("failed to flush MCP request: {error}"),
        })
}

fn disconnect_servers(servers: &mut BTreeMap<String, ServerState>) {
    for server in servers.values_mut() {
        if let Some(live) = server.live.as_mut() {
            let _ = live.process.kill();
        }
    }
}

fn transport_label(value: &Value) -> String {
    value
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("stdio")
        .to_owned()
}

fn error_message(error: &ClawinError) -> String {
    match error {
        ClawinError::InvalidConfiguration { message }
        | ClawinError::ModelDriver { message }
        | ClawinError::EngineProtocol { message } => message.clone(),
        ClawinError::UnknownCommand { name } => format!("unknown command: {name}"),
        ClawinError::InvalidCommandInvocation { message } => message.clone(),
        ClawinError::UnknownTool { name } => format!("unknown tool: {name}"),
        ClawinError::ToolInputInvalid { message, .. } => message.clone(),
        ClawinError::ToolExecution { message, .. } => message.clone(),
        ClawinError::NotImplemented { subsystem } => format!("{subsystem} is not implemented yet"),
    }
}

fn expand_env_vars(input: &str) -> String {
    let mut expanded = String::new();
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch != '$' {
            expanded.push(ch);
            continue;
        }

        match chars.peek().copied() {
            Some('{') => {
                chars.next();
                let mut name = String::new();
                for next in chars.by_ref() {
                    if next == '}' {
                        break;
                    }
                    name.push(next);
                }
                if name.is_empty() {
                    expanded.push('$');
                    expanded.push('{');
                    expanded.push('}');
                } else if let Ok(value) = std::env::var(&name) {
                    expanded.push_str(&value);
                } else {
                    expanded.push_str("${");
                    expanded.push_str(&name);
                    expanded.push('}');
                }
            }
            Some(next) if next.is_ascii_alphanumeric() || next == '_' => {
                let mut name = String::new();
                while let Some(next) = chars.peek().copied() {
                    if next.is_ascii_alphanumeric() || next == '_' {
                        chars.next();
                        name.push(next);
                    } else {
                        break;
                    }
                }
                if let Ok(value) = std::env::var(&name) {
                    expanded.push_str(&value);
                } else {
                    expanded.push('$');
                    expanded.push_str(&name);
                }
            }
            _ => expanded.push('$'),
        }
    }

    expanded
}
