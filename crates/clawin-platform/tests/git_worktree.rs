// Phase 7A tests continue under DIFF-2026-001: git-backed worktree behavior is exposed through Clawin-owned platform abstractions.

use std::path::PathBuf;

use clawin_platform::FakeGitWorktreeAdapter;

#[test]
fn fake_git_worktree_adapter_tracks_worktrees_and_dirty_state() {
    let repo_root = PathBuf::from("/repo");
    let worktree_path = PathBuf::from("/repo/.clawin/worktrees/feature-a");
    let adapter = FakeGitWorktreeAdapter::new();

    adapter.register_repository(repo_root.clone(), vec![repo_root.clone()]);
    assert_eq!(
        adapter
            .canonical_git_root(&repo_root)
            .expect("git root lookup should succeed"),
        Some(repo_root.clone())
    );

    let created = adapter
        .create_worktree(&repo_root, &worktree_path, "worktree-feature-a")
        .expect("worktree should be created");
    assert_eq!(created.path(), worktree_path);
    assert_eq!(created.branch(), Some("worktree-feature-a"));

    let listed = adapter
        .list_worktrees(&repo_root)
        .expect("worktree listing should succeed");
    assert_eq!(listed.len(), 2);

    adapter
        .set_dirty(&worktree_path, true)
        .expect("dirty state should update");
    assert!(
        adapter
            .is_dirty(&worktree_path)
            .expect("dirty lookup should work")
    );

    adapter
        .remove_worktree(&repo_root, &worktree_path, "worktree-feature-a", true)
        .expect("worktree should be removed");
    let listed = adapter
        .list_worktrees(&repo_root)
        .expect("worktree listing should succeed");
    assert_eq!(listed.len(), 1);
}
