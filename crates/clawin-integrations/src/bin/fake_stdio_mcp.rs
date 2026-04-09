use std::io::{self, BufReader, Read, Write};
use std::thread;
use std::time::Duration;

use serde_json::{Value, json};

fn main() -> io::Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = BufReader::new(stdin.lock());
    let mut writer = stdout.lock();
    let server = FakeServer::from_env();

    while let Some(message) = read_message(&mut reader)? {
        if let Some(response) = server.handle(message) {
            write_message(&mut writer, &response)?;
            writer.flush()?;
        }
    }

    Ok(())
}

#[derive(Clone, Debug)]
struct FakeServer {
    tool_name: String,
    resource_uri: String,
    binary_resource: bool,
    fail_initialize: bool,
    fail_tool_call: bool,
    delay_ms: u64,
}

impl FakeServer {
    fn from_env() -> Self {
        Self {
            tool_name: std::env::var("CLAWIN_FAKE_MCP_TOOL_NAME")
                .unwrap_or_else(|_| "echo".to_owned()),
            resource_uri: std::env::var("CLAWIN_FAKE_MCP_RESOURCE_URI")
                .unwrap_or_else(|_| "memo://alpha".to_owned()),
            binary_resource: env_flag("CLAWIN_FAKE_MCP_BINARY_RESOURCE"),
            fail_initialize: env_flag("CLAWIN_FAKE_MCP_FAIL_INITIALIZE"),
            fail_tool_call: env_flag("CLAWIN_FAKE_MCP_FAIL_TOOL_CALL"),
            delay_ms: std::env::var("CLAWIN_FAKE_MCP_DELAY_MS")
                .ok()
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(0),
        }
    }

    fn handle(&self, message: Value) -> Option<Value> {
        if self.delay_ms > 0 {
            thread::sleep(Duration::from_millis(self.delay_ms));
        }

        let method = message.get("method").and_then(Value::as_str)?;
        let request_id = message.get("id").cloned();

        match method {
            "notifications/initialized" => None,
            "initialize" => Some(self.initialize_response(request_id)),
            "tools/list" => Some(ok_response(
                request_id,
                json!({
                    "tools": [
                        {
                            "name": self.tool_name,
                            "description": "Fake MCP echo tool",
                            "inputSchema": {
                                "type": "object",
                                "properties": {
                                    "text": { "type": "string" }
                                }
                            }
                        }
                    ]
                }),
            )),
            "tools/call" => Some(self.tool_call_response(request_id, &message)),
            "resources/list" => Some(ok_response(
                request_id,
                json!({
                    "resources": [
                        {
                            "uri": self.resource_uri,
                            "name": "alpha",
                            "mimeType": if self.binary_resource {
                                "application/octet-stream"
                            } else {
                                "text/plain"
                            }
                        }
                    ]
                }),
            )),
            "resources/read" => Some(self.resource_read_response(request_id, &message)),
            _ => Some(error_response(
                request_id,
                -32601,
                format!("unsupported method: {method}"),
            )),
        }
    }

    fn initialize_response(&self, request_id: Option<Value>) -> Value {
        if self.fail_initialize {
            error_response(request_id, -32000, "initialize failed".to_owned())
        } else {
            ok_response(
                request_id,
                json!({
                    "protocolVersion": "2025-03-26",
                    "capabilities": {
                        "tools": {},
                        "resources": {}
                    },
                    "serverInfo": {
                        "name": "fake-stdio-mcp",
                        "version": env!("CARGO_PKG_VERSION")
                    }
                }),
            )
        }
    }

    fn tool_call_response(&self, request_id: Option<Value>, message: &Value) -> Value {
        if self.fail_tool_call {
            return error_response(request_id, -32001, "tool call failed".to_owned());
        }

        let params = message.get("params").cloned().unwrap_or(Value::Null);
        let text = params
            .get("arguments")
            .and_then(|arguments| arguments.get("text"))
            .and_then(Value::as_str)
            .unwrap_or("echo from fake server");

        ok_response(
            request_id,
            json!({
                "content": [
                    {
                        "type": "text",
                        "text": text
                    }
                ]
            }),
        )
    }

    fn resource_read_response(&self, request_id: Option<Value>, message: &Value) -> Value {
        let params = message.get("params").cloned().unwrap_or(Value::Null);
        let uri = params
            .get("uri")
            .and_then(Value::as_str)
            .unwrap_or(self.resource_uri.as_str());

        let contents = if self.binary_resource {
            json!([
                {
                    "uri": uri,
                    "mimeType": "application/octet-stream",
                    "blob": "AAEC"
                }
            ])
        } else {
            json!([
                {
                    "uri": uri,
                    "mimeType": "text/plain",
                    "text": "hello from fake stdio mcp"
                }
            ])
        };

        ok_response(request_id, json!({ "contents": contents }))
    }
}

fn env_flag(name: &str) -> bool {
    std::env::var(name)
        .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(false)
}

fn read_message(reader: &mut impl Read) -> io::Result<Option<Value>> {
    let mut header_bytes = Vec::new();
    let mut byte = [0_u8; 1];

    loop {
        let read = reader.read(&mut byte)?;
        if read == 0 {
            if header_bytes.is_empty() {
                return Ok(None);
            }
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "unexpected EOF while reading headers",
            ));
        }
        header_bytes.push(byte[0]);
        if header_bytes.ends_with(b"\r\n\r\n") {
            break;
        }
    }

    let headers = String::from_utf8(header_bytes).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("invalid UTF-8 in headers: {error}"),
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
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing Content-Length"))?;

    let mut payload = vec![0_u8; content_length];
    reader.read_exact(&mut payload)?;
    serde_json::from_slice(&payload).map(Some).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("invalid JSON payload: {error}"),
        )
    })
}

fn write_message(writer: &mut impl Write, value: &Value) -> io::Result<()> {
    let payload = serde_json::to_vec(value).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("failed to encode JSON response: {error}"),
        )
    })?;
    write!(writer, "Content-Length: {}\r\n\r\n", payload.len())?;
    writer.write_all(&payload)
}

fn ok_response(request_id: Option<Value>, result: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": request_id.unwrap_or(Value::Null),
        "result": result
    })
}

fn error_response(request_id: Option<Value>, code: i64, message: String) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": request_id.unwrap_or(Value::Null),
        "error": {
            "code": code,
            "message": message
        }
    })
}
