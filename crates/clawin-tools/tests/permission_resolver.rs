use std::fs;
use std::path::PathBuf;

use clawin_core::{
    PermissionBehavior, PermissionDecision, PermissionMode, PermissionResolver,
    PermissionResolverFuture, RuntimeCapabilities, SessionId, SessionRuntime, ToolCall,
};
use clawin_tools::builtin_tool_registry;
use serde_json::json;
use tempfile::TempDir;

#[tokio::test]
async fn allow_resolver_executes_asked_tool_call() {
    let harness = ToolHarness::new();
    let outside_file = harness.tempdir.path().join("outside.txt");
    fs::write(&outside_file, "secret").expect("outside file should be written");

    let execution = builtin_tool_registry()
        .execute_with_resolver(
            ToolCall::new(
                "toolu_allow",
                "file_read",
                json!({
                    "file_path": outside_file
                }),
            ),
            &harness.runtime(),
            &AllowResolver,
        )
        .await
        .expect("allow resolver should let file_read continue");

    assert_eq!(execution.permission_behavior, PermissionBehavior::Allow);
    assert!(!execution.result.is_error);
    assert_eq!(execution.result.content["content"], "secret");
}

#[tokio::test]
async fn deny_resolver_returns_structured_permission_denied_result() {
    let harness = ToolHarness::new();
    let outside_file = harness.tempdir.path().join("outside.txt");
    fs::write(&outside_file, "secret").expect("outside file should be written");

    let execution = builtin_tool_registry()
        .execute_with_resolver(
            ToolCall::new(
                "toolu_deny",
                "file_read",
                json!({
                    "file_path": outside_file
                }),
            ),
            &harness.runtime(),
            &DenyResolver,
        )
        .await
        .expect("deny resolver should return a structured tool error");

    assert_eq!(execution.permission_behavior, PermissionBehavior::Deny);
    assert!(execution.result.is_error);
    assert_eq!(execution.result.content["code"], "permission_denied");
    assert_eq!(
        execution.result.content["message"],
        "host denied tool access"
    );
}

struct AllowResolver;

impl PermissionResolver for AllowResolver {
    fn resolve(
        &self,
        _call: &ToolCall,
        _decision: PermissionDecision,
    ) -> PermissionResolverFuture<'_> {
        Box::pin(async { Ok(PermissionDecision::new(PermissionBehavior::Allow, None)) })
    }
}

struct DenyResolver;

impl PermissionResolver for DenyResolver {
    fn resolve(
        &self,
        _call: &ToolCall,
        _decision: PermissionDecision,
    ) -> PermissionResolverFuture<'_> {
        Box::pin(async {
            Ok(PermissionDecision::new(
                PermissionBehavior::Deny,
                Some("host denied tool access".to_owned()),
            ))
        })
    }
}

struct ToolHarness {
    tempdir: TempDir,
    project_root: PathBuf,
}

impl ToolHarness {
    fn new() -> Self {
        let tempdir = tempfile::tempdir().expect("tempdir should exist");
        let project_root = tempdir.path().join("project");
        fs::create_dir_all(&project_root).expect("project root should exist");
        Self {
            tempdir,
            project_root,
        }
    }

    fn runtime(&self) -> SessionRuntime {
        SessionRuntime::new(
            SessionId::from_static("permission-test"),
            RuntimeCapabilities::new(false, false),
            self.project_root.clone(),
            self.project_root.clone(),
            PermissionMode::Default,
        )
    }
}
