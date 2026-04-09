use std::fs;
use std::sync::Arc;

use clawin_core::{
    ClawinError, ClawinResult, PersistedWorktreeSession, SessionRuntime, WorktreeExitAction,
    WorktreeManager,
};
use clawin_platform::{GitWorktreeAdapter, PathPolicy};

const WORKTREES_DIRECTORY_NAME: &str = "worktrees";

#[derive(Clone)]
pub struct GitSessionWorktreeManager<P, G> {
    path_policy: P,
    git: Arc<G>,
}

impl<P, G> GitSessionWorktreeManager<P, G> {
    pub fn new(path_policy: P, git: Arc<G>) -> Self {
        Self { path_policy, git }
    }
}

impl<P, G> WorktreeManager for GitSessionWorktreeManager<P, G>
where
    P: PathPolicy + Send + Sync + 'static,
    G: GitWorktreeAdapter + Send + Sync + 'static,
{
    fn enter_worktree(
        &self,
        runtime: &SessionRuntime,
        name: Option<&str>,
    ) -> ClawinResult<PersistedWorktreeSession> {
        let repo_root = self
            .git
            .canonical_git_root(runtime.canonical_project_root())
            .map_err(map_git_error)?
            .ok_or_else(|| ClawinError::InvalidConfiguration {
                message: "EnterWorktree requires a git repository".to_owned(),
            })?;
        let slug = slugify_worktree_name(name.unwrap_or(runtime.session_id().as_str()));
        let worktree_path = repo_root
            .join(self.path_policy.project_directory_name())
            .join(WORKTREES_DIRECTORY_NAME)
            .join(&slug);
        let branch_name = format!("worktree-{slug}");

        if let Some(parent) = worktree_path.parent() {
            fs::create_dir_all(parent).map_err(|error| ClawinError::InvalidConfiguration {
                message: format!(
                    "failed to create worktree parent directory {}: {error}",
                    parent.display()
                ),
            })?;
        }

        let worktree = self
            .git
            .create_worktree(&repo_root, &worktree_path, &branch_name)
            .map_err(map_git_error)?;
        let session = PersistedWorktreeSession::new(
            repo_root.clone(),
            worktree.path().to_path_buf(),
            worktree.branch().map(str::to_owned),
            true,
        );
        runtime.set_active_project_root(session.worktree_path.clone());
        runtime.set_current_cwd(session.worktree_path.clone());
        runtime.set_active_worktree(Some(session.clone()));
        Ok(session)
    }

    fn exit_worktree(
        &self,
        runtime: &SessionRuntime,
        action: WorktreeExitAction,
        discard_changes: bool,
    ) -> ClawinResult<Option<PersistedWorktreeSession>> {
        let Some(active) = runtime.active_worktree() else {
            return Ok(None);
        };

        if action == WorktreeExitAction::Remove
            && !discard_changes
            && self
                .git
                .is_dirty(&active.worktree_path)
                .map_err(map_git_error)?
        {
            return Err(ClawinError::InvalidConfiguration {
                message:
                    "cannot remove a dirty session-owned worktree without discard_changes = true"
                        .to_owned(),
            });
        }

        if action == WorktreeExitAction::Remove {
            let branch_name = active.branch.as_deref().unwrap_or("worktree-session");
            self.git
                .remove_worktree(
                    &active.canonical_project_root,
                    &active.worktree_path,
                    branch_name,
                    discard_changes,
                )
                .map_err(map_git_error)?;
        }

        runtime.set_active_project_root(runtime.canonical_project_root().to_path_buf());
        runtime.set_current_cwd(runtime.canonical_project_root().to_path_buf());
        runtime.set_active_worktree(None);
        Ok(Some(active))
    }
}

fn slugify_worktree_name(value: &str) -> String {
    let mut slug = String::with_capacity(value.len());
    let mut previous_was_separator = false;

    for ch in value.chars() {
        let mapped = match ch {
            'a'..='z' | '0'..='9' => Some(ch),
            'A'..='Z' => Some(ch.to_ascii_lowercase()),
            '-' | '_' => Some(ch),
            _ => Some('-'),
        };

        let Some(mapped) = mapped else {
            continue;
        };

        if mapped == '-' {
            if previous_was_separator {
                continue;
            }
            previous_was_separator = true;
        } else {
            previous_was_separator = false;
        }

        slug.push(mapped);
    }

    let slug = slug.trim_matches('-').trim_matches('_');
    if slug.is_empty() {
        "session".to_owned()
    } else {
        slug.to_owned()
    }
}

fn map_git_error(error: std::io::Error) -> ClawinError {
    ClawinError::InvalidConfiguration {
        message: format!("git worktree operation failed: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    use clawin_core::{PermissionMode, RuntimeCapabilities, SessionId, WorktreeManager};
    use clawin_platform::{FakeGitWorktreeAdapter, PathPolicy};
    use tempfile::TempDir;

    use super::GitSessionWorktreeManager;

    #[test]
    fn enter_worktree_creates_session_owned_worktree_and_updates_runtime() {
        let harness = WorktreeHarness::new();
        let git = Arc::new(FakeGitWorktreeAdapter::new());
        git.register_repository(
            harness.project_root.clone(),
            vec![harness.project_root.clone()],
        );
        let runtime = harness.runtime();
        let manager = GitSessionWorktreeManager::new(harness.path_policy, git.clone());

        let session = manager
            .enter_worktree(&runtime, Some("Feature A"))
            .expect("worktree should be created");

        assert!(session.session_owned);
        assert_eq!(runtime.active_project_root(), session.worktree_path);
        assert_eq!(
            git.list_worktrees(&harness.project_root)
                .expect("worktree listing should succeed")
                .len(),
            2
        );
    }

    #[test]
    fn removing_dirty_worktree_requires_explicit_discard() {
        let harness = WorktreeHarness::new();
        let git = Arc::new(FakeGitWorktreeAdapter::new());
        git.register_repository(
            harness.project_root.clone(),
            vec![harness.project_root.clone()],
        );
        let runtime = harness.runtime();
        let manager = GitSessionWorktreeManager::new(harness.path_policy, git.clone());
        let session = manager
            .enter_worktree(&runtime, Some("dirty"))
            .expect("worktree should be created");
        git.set_dirty(&session.worktree_path, true)
            .expect("dirty flag should update");

        let error = manager
            .exit_worktree(&runtime, clawin_core::WorktreeExitAction::Remove, false)
            .expect_err("dirty worktree removal should fail");

        assert!(matches!(
            error,
            clawin_core::ClawinError::InvalidConfiguration { ref message }
                if message.contains("discard_changes")
        ));
    }

    struct WorktreeHarness {
        _tempdir: TempDir,
        project_root: PathBuf,
        path_policy: TestPathPolicy,
    }

    impl WorktreeHarness {
        fn new() -> Self {
            let tempdir = tempfile::tempdir().expect("tempdir should exist");
            let project_root = tempdir.path().join("repo");
            std::fs::create_dir_all(&project_root).expect("project root should exist");
            Self {
                _tempdir: tempdir,
                project_root,
                path_policy: TestPathPolicy,
            }
        }

        fn runtime(&self) -> clawin_core::SessionRuntime {
            clawin_core::SessionRuntime::new(
                SessionId::from_static("worktree-session"),
                RuntimeCapabilities::new(false, false),
                self.project_root.clone(),
                self.project_root.clone(),
                PermissionMode::Default,
            )
        }
    }

    #[derive(Clone, Copy)]
    struct TestPathPolicy;

    impl PathPolicy for TestPathPolicy {
        fn home_dir(&self) -> Option<PathBuf> {
            None
        }

        fn normalize_for_config_key(&self, path: &Path) -> String {
            path.to_string_lossy().replace('\\', "/")
        }

        fn project_directory_name(&self) -> &'static str {
            ".clawin"
        }

        fn project_manifest_name(&self) -> &'static str {
            "CLAWIN.md"
        }
    }
}
