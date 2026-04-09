use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use clawin_core::{
    ClawinError, ClawinResult, ConversationMessage, PersistedWorktreeSession, RestoredSession,
    ResumeInterruptionState, ResumeQuery, SessionPreview, SessionRuntime, SessionStore,
};
use clawin_platform::{GitWorktreeAdapter, PathPolicy};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::ClawinPaths;

pub const SESSION_SCHEMA_VERSION: u32 = 1;

/// JSONL-backed session persistence used by Phase 7A bootstrap, REPL, and resume flows.
#[derive(Clone, Debug)]
pub struct JsonlSessionStore<P, G> {
    paths: ClawinPaths,
    path_policy: P,
    git: Arc<G>,
}

impl<P, G> JsonlSessionStore<P, G>
where
    P: PathPolicy + Clone,
    G: GitWorktreeAdapter,
{
    /// Create a new JSONL session store bound to the current startup path snapshot.
    pub fn new(paths: ClawinPaths, path_policy: P, git: Arc<G>) -> Self {
        Self {
            paths,
            path_policy,
            git,
        }
    }

    /// Initialize a new session transcript file with a stable header entry.
    pub fn initialize_session(&self, runtime: &SessionRuntime) -> ClawinResult<()> {
        let transcript_path = self.ensure_transcript_path(runtime);
        self.ensure_parent_dir(&transcript_path)?;
        let header = self.session_header(runtime);
        fs::write(&transcript_path, format!("{}\n", serialize_entry(&header)?)).map_err(|error| {
            ClawinError::InvalidConfiguration {
                message: format!(
                    "failed to initialize session transcript {}: {error}",
                    transcript_path.display()
                ),
            }
        })
    }

    /// Persist the last submitted prompt for resume list previews.
    pub fn save_last_prompt(&self, runtime: &SessionRuntime, prompt: &str) -> ClawinResult<()> {
        self.append_entry(
            runtime,
            &SessionEntry::LastPrompt {
                content: prompt.to_owned(),
            },
        )
    }

    /// Append a transcript message entry.
    pub fn append_message(
        &self,
        runtime: &SessionRuntime,
        message: &ConversationMessage,
    ) -> ClawinResult<()> {
        self.append_entry(
            runtime,
            &SessionEntry::Message {
                message: message.clone(),
            },
        )
    }

    /// Persist the current worktree snapshot for resume flows.
    pub fn save_worktree_state(
        &self,
        runtime: &SessionRuntime,
        worktree: Option<&PersistedWorktreeSession>,
    ) -> ClawinResult<()> {
        self.append_entry(
            runtime,
            &SessionEntry::WorktreeState {
                state: worktree.cloned(),
            },
        )
    }

    /// Resolve a session restore request within the current project/worktree scope.
    pub fn resolve_resume(
        &self,
        runtime: &SessionRuntime,
        query: ResumeQuery,
    ) -> ClawinResult<Option<RestoredSession>> {
        match query {
            ResumeQuery::Continue => self
                .list_recent_sessions(runtime)?
                .into_iter()
                .next()
                .map(|preview| self.load_session_file(&preview.transcript_path))
                .transpose(),
            ResumeQuery::Exact(session_id) => {
                let matches = self
                    .session_files_in_scope(runtime)?
                    .into_iter()
                    .filter(|path| session_file_matches_id(path, &session_id))
                    .collect::<Vec<_>>();
                resolve_single_match(matches, |path| self.load_session_file(path))
            }
            ResumeQuery::Search(term) => {
                let matches = self
                    .list_recent_sessions(runtime)?
                    .into_iter()
                    .filter(|preview| {
                        preview.session_id.as_str().contains(&term)
                            || preview
                                .last_prompt
                                .as_deref()
                                .is_some_and(|prompt| prompt.contains(&term))
                    })
                    .map(|preview| preview.transcript_path)
                    .collect::<Vec<_>>();
                resolve_single_match(matches, |path| self.load_session_file(path))
            }
            ResumeQuery::Path(path) => self.load_session_file(&path).map(Some),
        }
    }

    /// List recent sessions visible from the current active project and same-repo worktrees.
    pub fn list_recent_sessions(
        &self,
        runtime: &SessionRuntime,
    ) -> ClawinResult<Vec<SessionPreview>> {
        let mut sessions = self
            .session_files_in_scope(runtime)?
            .into_iter()
            .map(|path| {
                let metadata =
                    fs::metadata(&path).map_err(|error| ClawinError::InvalidConfiguration {
                        message: format!(
                            "failed to stat session transcript {}: {error}",
                            path.display()
                        ),
                    })?;
                let modified = metadata.modified().ok();
                let preview = self.load_session_preview(&path)?;
                Ok((modified, preview))
            })
            .collect::<ClawinResult<Vec<_>>>()?;
        sessions.sort_by(|left, right| right.0.cmp(&left.0));
        Ok(sessions.into_iter().map(|(_, preview)| preview).collect())
    }

    fn append_entry(&self, runtime: &SessionRuntime, entry: &SessionEntry) -> ClawinResult<()> {
        let transcript_path = self.ensure_transcript_path(runtime);
        self.ensure_parent_dir(&transcript_path)?;
        if !transcript_path.exists() {
            let header = self.session_header(runtime);
            fs::write(&transcript_path, format!("{}\n", serialize_entry(&header)?)).map_err(
                |error| ClawinError::InvalidConfiguration {
                    message: format!(
                        "failed to initialize session transcript {}: {error}",
                        transcript_path.display()
                    ),
                },
            )?;
        }
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&transcript_path)
            .map_err(|error| ClawinError::InvalidConfiguration {
                message: format!(
                    "failed to open session transcript {}: {error}",
                    transcript_path.display()
                ),
            })?;
        writeln!(file, "{}", serialize_entry(entry)?).map_err(|error| {
            ClawinError::InvalidConfiguration {
                message: format!(
                    "failed to append session transcript {}: {error}",
                    transcript_path.display()
                ),
            }
        })
    }

    fn ensure_transcript_path(&self, runtime: &SessionRuntime) -> PathBuf {
        if let Some(path) = runtime.session_transcript_path() {
            return path;
        }

        let path = self
            .default_transcript_path(runtime.active_project_root(), runtime.session_id().as_str());
        runtime.set_session_transcript_path(path.clone());
        path
    }

    fn default_transcript_path(&self, active_project_root: PathBuf, session_id: &str) -> PathBuf {
        self.session_project_directory(active_project_root)
            .join(format!("{session_id}.jsonl"))
    }

    fn session_project_directory(&self, active_project_root: PathBuf) -> PathBuf {
        self.paths.projects_root().join(
            self.path_policy
                .sanitize_for_session_dir(&active_project_root),
        )
    }

    fn ensure_parent_dir(&self, path: &Path) -> ClawinResult<()> {
        let Some(parent) = path.parent() else {
            return Ok(());
        };
        fs::create_dir_all(parent).map_err(|error| ClawinError::InvalidConfiguration {
            message: format!(
                "failed to create session directory {}: {error}",
                parent.display()
            ),
        })
    }

    fn session_files_in_scope(&self, runtime: &SessionRuntime) -> ClawinResult<Vec<PathBuf>> {
        let mut directories = vec![self.session_project_directory(runtime.active_project_root())];
        if let Some(repo_root) = self
            .git
            .canonical_git_root(runtime.canonical_project_root())
            .map_err(map_git_error)?
        {
            for worktree in self.git.list_worktrees(&repo_root).map_err(map_git_error)? {
                let directory = self.session_project_directory(worktree.path().to_path_buf());
                if !directories.iter().any(|existing| existing == &directory) {
                    directories.push(directory);
                }
            }
        }

        let mut files = Vec::new();
        for directory in directories {
            if !directory.exists() {
                continue;
            }
            let entries =
                fs::read_dir(&directory).map_err(|error| ClawinError::InvalidConfiguration {
                    message: format!(
                        "failed to read session directory {}: {error}",
                        directory.display()
                    ),
                })?;
            for entry in entries {
                let path = entry
                    .map_err(|error| ClawinError::InvalidConfiguration {
                        message: format!(
                            "failed to read session directory entry {}: {error}",
                            directory.display()
                        ),
                    })?
                    .path();
                if path
                    .extension()
                    .is_some_and(|extension| extension == "jsonl")
                {
                    files.push(path);
                }
            }
        }
        Ok(files)
    }

    fn load_session_preview(&self, transcript_path: &Path) -> ClawinResult<SessionPreview> {
        let loaded = load_entries(transcript_path)?;
        Ok(SessionPreview {
            session_id: loaded.header.session_id,
            transcript_path: transcript_path.to_path_buf(),
            last_prompt: loaded.last_prompt,
            active_project_root: loaded
                .worktree_state
                .as_ref()
                .map(|worktree| worktree.worktree_path.clone())
                .unwrap_or(loaded.header.active_project_root),
        })
    }

    fn load_session_file(&self, transcript_path: &Path) -> ClawinResult<RestoredSession> {
        let loaded = load_entries(transcript_path)?;
        let interruption_state =
            detect_interruption(loaded.last_prompt.as_deref(), &loaded.messages);
        let active_project_root = loaded
            .worktree_state
            .as_ref()
            .map(|worktree| worktree.worktree_path.clone())
            .unwrap_or_else(|| loaded.header.active_project_root.clone());
        Ok(RestoredSession {
            session_id: loaded.header.session_id,
            transcript_path: transcript_path.to_path_buf(),
            canonical_project_root: loaded.header.canonical_project_root,
            active_project_root,
            transcript: loaded.messages,
            last_prompt: loaded.last_prompt,
            worktree_state: loaded.worktree_state,
            interruption_state,
        })
    }

    fn session_header(&self, runtime: &SessionRuntime) -> SessionEntry {
        SessionEntry::SessionHeader {
            schema_version: SESSION_SCHEMA_VERSION,
            session_id: runtime.session_id().clone(),
            launch_cwd: runtime.launch_cwd().to_path_buf(),
            canonical_project_root: runtime.canonical_project_root().to_path_buf(),
            active_project_root: runtime.active_project_root(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum SessionEntry {
    SessionHeader {
        schema_version: u32,
        session_id: clawin_core::SessionId,
        launch_cwd: PathBuf,
        canonical_project_root: PathBuf,
        active_project_root: PathBuf,
    },
    Message {
        message: ConversationMessage,
    },
    LastPrompt {
        content: String,
    },
    WorktreeState {
        state: Option<PersistedWorktreeSession>,
    },
}

#[derive(Debug)]
struct LoadedEntries {
    header: SessionHeader,
    messages: Vec<ConversationMessage>,
    last_prompt: Option<String>,
    worktree_state: Option<PersistedWorktreeSession>,
}

#[derive(Debug)]
struct SessionHeader {
    session_id: clawin_core::SessionId,
    canonical_project_root: PathBuf,
    active_project_root: PathBuf,
}

fn load_entries(path: &Path) -> ClawinResult<LoadedEntries> {
    let file = fs::File::open(path).map_err(|error| ClawinError::InvalidConfiguration {
        message: format!(
            "failed to open session transcript {}: {error}",
            path.display()
        ),
    })?;
    let reader = BufReader::new(file);
    let mut header = None;
    let mut messages = Vec::new();
    let mut last_prompt = None;
    let mut worktree_state = None;

    for line in reader.lines() {
        let line = line.map_err(|error| ClawinError::InvalidConfiguration {
            message: format!(
                "failed to read session transcript {}: {error}",
                path.display()
            ),
        })?;
        if line.trim().is_empty() {
            continue;
        }
        let value = serde_json::from_str::<Value>(&line).map_err(|error| {
            ClawinError::InvalidConfiguration {
                message: format!("invalid session jsonl entry in {}: {error}", path.display()),
            }
        })?;
        let Some(entry_type) = value.get("type").and_then(Value::as_str) else {
            return Err(ClawinError::InvalidConfiguration {
                message: format!("session entry missing type in {}", path.display()),
            });
        };

        match entry_type {
            "session_header" => {
                match serde_json::from_value::<SessionEntry>(value).map_err(|error| {
                    ClawinError::InvalidConfiguration {
                        message: format!(
                            "invalid session header entry in {}: {error}",
                            path.display()
                        ),
                    }
                })? {
                    SessionEntry::SessionHeader {
                        schema_version,
                        session_id,
                        canonical_project_root,
                        active_project_root,
                        ..
                    } => {
                        if schema_version != SESSION_SCHEMA_VERSION {
                            return Err(ClawinError::InvalidConfiguration {
                                message: format!(
                                    "unsupported session schema version {} in {}",
                                    schema_version,
                                    path.display()
                                ),
                            });
                        }
                        header = Some(SessionHeader {
                            session_id,
                            canonical_project_root,
                            active_project_root,
                        });
                    }
                    _ => unreachable!("session entry tag should match parsed variant"),
                }
            }
            "message" => match serde_json::from_value::<SessionEntry>(value).map_err(|error| {
                ClawinError::InvalidConfiguration {
                    message: format!(
                        "invalid session message entry in {}: {error}",
                        path.display()
                    ),
                }
            })? {
                SessionEntry::Message { message } => messages.push(message),
                _ => unreachable!("session entry tag should match parsed variant"),
            },
            "last_prompt" => {
                match serde_json::from_value::<SessionEntry>(value).map_err(|error| {
                    ClawinError::InvalidConfiguration {
                        message: format!(
                            "invalid last prompt entry in {}: {error}",
                            path.display()
                        ),
                    }
                })? {
                    SessionEntry::LastPrompt { content } => last_prompt = Some(content),
                    _ => unreachable!("session entry tag should match parsed variant"),
                }
            }
            "worktree_state" => {
                match serde_json::from_value::<SessionEntry>(value).map_err(|error| {
                    ClawinError::InvalidConfiguration {
                        message: format!(
                            "invalid worktree state entry in {}: {error}",
                            path.display()
                        ),
                    }
                })? {
                    SessionEntry::WorktreeState { state } => worktree_state = state,
                    _ => unreachable!("session entry tag should match parsed variant"),
                }
            }
            _ => continue,
        }
    }

    let Some(header) = header else {
        return Err(ClawinError::InvalidConfiguration {
            message: format!("missing session header in {}", path.display()),
        });
    };

    Ok(LoadedEntries {
        header,
        messages,
        last_prompt,
        worktree_state,
    })
}

fn serialize_entry(entry: &SessionEntry) -> ClawinResult<String> {
    serde_json::to_string(entry).map_err(|error| ClawinError::InvalidConfiguration {
        message: format!("failed to serialize session entry: {error}"),
    })
}

fn detect_interruption(
    last_prompt: Option<&str>,
    messages: &[ConversationMessage],
) -> ResumeInterruptionState {
    if last_prompt.is_none() {
        return ResumeInterruptionState::None;
    }

    match messages.last() {
        None | Some(ConversationMessage::User { .. }) => ResumeInterruptionState::InterruptedPrompt,
        _ => ResumeInterruptionState::None,
    }
}

fn session_file_matches_id(path: &Path, session_id: &str) -> bool {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .is_some_and(|stem| stem == session_id)
}

fn resolve_single_match<T, F>(matches: Vec<PathBuf>, load: F) -> ClawinResult<Option<T>>
where
    F: FnOnce(&Path) -> ClawinResult<T>,
{
    match matches.as_slice() {
        [] => Ok(None),
        [single] => load(single).map(Some),
        _ => Err(ClawinError::InvalidConfiguration {
            message: "resume query matched multiple sessions".to_owned(),
        }),
    }
}

fn map_git_error(error: std::io::Error) -> ClawinError {
    ClawinError::InvalidConfiguration {
        message: format!("git worktree lookup failed: {error}"),
    }
}

impl<P, G> SessionStore for JsonlSessionStore<P, G>
where
    P: PathPolicy + Clone + Send + Sync + 'static,
    G: GitWorktreeAdapter + Send + Sync + 'static,
{
    fn initialize_session(&self, runtime: &SessionRuntime) -> ClawinResult<()> {
        Self::initialize_session(self, runtime)
    }

    fn save_last_prompt(&self, runtime: &SessionRuntime, prompt: &str) -> ClawinResult<()> {
        Self::save_last_prompt(self, runtime, prompt)
    }

    fn append_message(
        &self,
        runtime: &SessionRuntime,
        message: &ConversationMessage,
    ) -> ClawinResult<()> {
        Self::append_message(self, runtime, message)
    }

    fn save_worktree_state(
        &self,
        runtime: &SessionRuntime,
        worktree: Option<&PersistedWorktreeSession>,
    ) -> ClawinResult<()> {
        Self::save_worktree_state(self, runtime, worktree)
    }

    fn list_recent_sessions(&self, runtime: &SessionRuntime) -> ClawinResult<Vec<SessionPreview>> {
        Self::list_recent_sessions(self, runtime)
    }

    fn resolve_resume(
        &self,
        runtime: &SessionRuntime,
        query: ResumeQuery,
    ) -> ClawinResult<Option<RestoredSession>> {
        Self::resolve_resume(self, runtime, query)
    }
}
