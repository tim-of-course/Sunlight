use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use sunlight_core::records::{canonical_json_bytes, parse_json_record, JsonValue};
use sunlight_core::repo_state::{
    durable_publish_json_bytes, parse_real_execution_snapshot_value,
    parse_real_projection_snapshot_value, real_execution_snapshot_json_value,
    real_projection_snapshot_json_value, RealExecutionSnapshot, RealProjectionSnapshot,
    RealRepoState, RepoStateError,
};
use sunlight_core::resolver::TopicRevisionSelection;

use super::{real_execution_snapshot_record_json, real_now_id, real_projection_snapshot_json};

const LOCK_TIMEOUT: Duration = Duration::from_secs(10);
const LOCK_POLL: Duration = Duration::from_millis(10);
#[cfg(debug_assertions)]
const FAILPOINT_ENV: &str = "SUNLIGHT_INTERNAL_TEST_EXECUTION_STORE_FAILPOINT";
static TOKEN_NONCE: AtomicU64 = AtomicU64::new(1);

#[derive(Debug)]
pub(crate) struct ExecutionStore {
    repo_root: PathBuf,
}

#[derive(Debug)]
pub(crate) struct ExecutionReservation {
    pub sequence: usize,
    path: PathBuf,
}

impl Drop for ExecutionReservation {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

#[derive(Debug)]
pub(crate) struct ExecutionLock {
    _file: File,
}

#[derive(Debug)]
struct StoredExecution {
    snapshot: RealExecutionSnapshot,
    runner_instance_token: String,
}

#[derive(Debug)]
struct StoredProjection {
    execution_id: String,
    snapshot: RealProjectionSnapshot,
}

impl ExecutionStore {
    pub(crate) fn new(repo_root: &Path) -> Self {
        Self {
            repo_root: repo_root.to_path_buf(),
        }
    }

    pub(crate) fn reserve(
        &self,
        legacy: &RealRepoState,
    ) -> Result<ExecutionReservation, RepoStateError> {
        let root = self.reservation_root();
        fs::create_dir_all(&root).map_err(|error| {
            io_error(
                &root,
                "failed to create execution reservation storage",
                error,
            )
        })?;
        let standalone_ids = self
            .standalone_executions()?
            .into_keys()
            .collect::<BTreeSet<_>>();
        let standalone_projection_ids = self
            .standalone_projections()?
            .into_keys()
            .collect::<BTreeSet<_>>();
        let mut sequence = 1_usize;
        loop {
            let execution_id = format!("exec_native_{sequence:04}");
            let projection_suffix = format!("native_{sequence:04}");
            let occupied = standalone_ids.contains(&execution_id)
                || legacy
                    .executions
                    .iter()
                    .any(|execution| execution.execution_id == execution_id)
                || standalone_projection_ids
                    .iter()
                    .any(|id| id.ends_with(&projection_suffix))
                || legacy.projections.iter().any(|projection| {
                    projection.purpose == "execution"
                        && projection.projection_id.ends_with(&projection_suffix)
                });
            if occupied {
                sequence += 1;
                continue;
            }
            let path = root.join(format!("{sequence:04}"));
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(_) => return Ok(ExecutionReservation { sequence, path }),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => sequence += 1,
                Err(error) => {
                    return Err(io_error(
                        &path,
                        "failed to reserve unique execution identity",
                        error,
                    ))
                }
            }
        }
    }

    pub(crate) fn acquire_lock(&self, execution_id: &str) -> Result<ExecutionLock, RepoStateError> {
        self.acquire_lock_inner(execution_id, true)?
            .ok_or_else(|| RepoStateError::WriterBusy {
                lock: self.lock_path(execution_id),
                timeout_ms: LOCK_TIMEOUT.as_millis().try_into().unwrap_or(u64::MAX),
            })
    }

    fn try_acquire_lock(
        &self,
        execution_id: &str,
    ) -> Result<Option<ExecutionLock>, RepoStateError> {
        self.acquire_lock_inner(execution_id, false)
    }

    fn acquire_lock_inner(
        &self,
        execution_id: &str,
        wait: bool,
    ) -> Result<Option<ExecutionLock>, RepoStateError> {
        let path = self.lock_path(execution_id);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                io_error(parent, "failed to create execution lock storage", error)
            })?;
        }
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(&path)
            .map_err(|error| io_error(&path, "failed to open execution lock", error))?;
        let started = std::time::Instant::now();
        loop {
            match file.try_lock() {
                Ok(()) => return Ok(Some(ExecutionLock { _file: file })),
                Err(fs::TryLockError::WouldBlock) if !wait => return Ok(None),
                Err(fs::TryLockError::WouldBlock) if started.elapsed() < LOCK_TIMEOUT => {
                    thread::sleep(LOCK_POLL)
                }
                Err(fs::TryLockError::WouldBlock) => return Ok(None),
                Err(fs::TryLockError::Error(error)) => {
                    return Err(io_error(&path, "failed to lock execution record", error))
                }
            }
        }
    }

    pub(crate) fn runner_instance_token(&self, execution_id: &str) -> String {
        let nonce = TOKEN_NONCE.fetch_add(1, Ordering::Relaxed);
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        sunlight_core::repo_state::real_content_hash(
            format!(
                "{}\0{}\0{}\0{}",
                self.repo_root.display(),
                execution_id,
                std::process::id(),
                now ^ u128::from(nonce)
            )
            .as_bytes(),
        )
    }

    pub(crate) fn has_records(&self) -> Result<bool, RepoStateError> {
        Ok(!self.standalone_executions()?.is_empty() || !self.standalone_projections()?.is_empty())
    }

    pub(crate) fn has_projection(&self, projection_id: &str) -> Result<bool, RepoStateError> {
        Ok(self.read_stored_projection(projection_id)?.is_some())
    }

    pub(crate) fn publish_start(
        &self,
        state: &RealRepoState,
        projection: &RealProjectionSnapshot,
        execution: &RealExecutionSnapshot,
        runner_instance_token: &str,
    ) -> Result<(), RepoStateError> {
        self.failpoint("before_projection_publication")?;
        if self
            .read_stored_execution(&execution.execution_id)?
            .is_some()
        {
            return Err(invalid_state(
                self.execution_path(&execution.execution_id),
                "execution identity is already bound to durable facts",
            ));
        }
        self.publish_projection(state, &execution.execution_id, projection)?;
        self.failpoint("between_projection_and_execution_publication")?;
        let bytes = execution_record_bytes(state, execution, runner_instance_token)?;
        durable_publish_json_bytes(
            &self.repo_root,
            &self.execution_path(&execution.execution_id),
            &bytes,
            "execution_store_running_publication",
        )?;
        self.failpoint("after_running_publication")
    }

    fn publish_projection(
        &self,
        state: &RealRepoState,
        execution_id: &str,
        projection: &RealProjectionSnapshot,
    ) -> Result<(), RepoStateError> {
        let path = self.projection_path(&projection.projection_id);
        if path.exists() {
            return Err(invalid_state(
                path,
                "execution projection identity is already bound to durable facts",
            ));
        }
        let bytes = projection_record_bytes(state, execution_id, projection)?;
        durable_publish_json_bytes(
            &self.repo_root,
            &self.projection_path(&projection.projection_id),
            &bytes,
            "execution_store_projection_publication",
        )
    }

    pub(crate) fn finalize(
        &self,
        state: &RealRepoState,
        execution: &RealExecutionSnapshot,
        runner_instance_token: &str,
        quarantine_projection: bool,
    ) -> Result<RealProjectionSnapshot, RepoStateError> {
        let stored = self
            .read_stored_execution(&execution.execution_id)?
            .ok_or_else(|| {
                invalid_state(
                    self.execution_path(&execution.execution_id),
                    "durable running execution is missing",
                )
            })?;
        if stored.runner_instance_token != runner_instance_token
            || stored.snapshot.status != "running"
            || stored.snapshot.projection_id != execution.projection_id
            || stored.snapshot.resolved_view_id != execution.resolved_view_id
            || stored.snapshot.tree_hash != execution.tree_hash
        {
            return Err(invalid_state(
                self.execution_path(&execution.execution_id),
                "terminal execution publication did not match the durable runner instance",
            ));
        }
        let mut projection = self
            .read_stored_projection(&execution.projection_id)?
            .filter(|projection| projection.execution_id == execution.execution_id)
            .map(|projection| projection.snapshot)
            .ok_or_else(|| {
                invalid_state(
                    self.projection_path(&execution.projection_id),
                    "durable execution projection is missing",
                )
            })?;
        if quarantine_projection {
            projection.retention_state = "quarantined".to_string();
            let bytes = projection_record_bytes(state, &execution.execution_id, &projection)?;
            durable_publish_json_bytes(
                &self.repo_root,
                &self.projection_path(&projection.projection_id),
                &bytes,
                "execution_store_projection_terminal_publication",
            )?;
        }
        let bytes = execution_record_bytes(state, execution, runner_instance_token)?;
        durable_publish_json_bytes(
            &self.repo_root,
            &self.execution_path(&execution.execution_id),
            &bytes,
            "execution_store_terminal_publication",
        )?;
        Ok(projection)
    }

    pub(crate) fn merge_into(&self, state: &mut RealRepoState) -> Result<(), RepoStateError> {
        for (execution_id, stored) in self.standalone_executions()? {
            state
                .executions
                .retain(|execution| execution.execution_id != execution_id);
            state.executions.push(stored.snapshot);
        }
        let visible_execution_ids = state
            .executions
            .iter()
            .map(|execution| execution.execution_id.clone())
            .collect::<BTreeSet<_>>();
        for (projection_id, mut stored) in self.standalone_projections()? {
            if !visible_execution_ids.contains(&stored.execution_id) {
                continue;
            }
            if stored.snapshot.entries.is_empty() {
                let resolved = state.resolve_view(
                    stored
                        .snapshot
                        .topic_frontier
                        .iter()
                        .map(|(topic_id, revision_id)| TopicRevisionSelection {
                            topic_id: topic_id.clone(),
                            revision_id: revision_id.clone(),
                        })
                        .collect(),
                );
                if resolved.result.conflict_free()
                    && resolved.result.resolved_view_id == stored.snapshot.resolved_view_id
                    && resolved
                        .result
                        .tree_identity
                        .as_ref()
                        .is_some_and(|tree| tree.tree_hash == stored.snapshot.tree_hash)
                {
                    stored.snapshot.entries = resolved.entries;
                }
            }
            state
                .projections
                .retain(|projection| projection.projection_id != projection_id);
            state.projections.push(stored.snapshot);
        }
        state
            .executions
            .sort_by(|left, right| left.execution_id.cmp(&right.execution_id));
        state
            .projections
            .sort_by(|left, right| left.projection_id.cmp(&right.projection_id));
        Ok(())
    }

    pub(crate) fn strip_standalone_from(
        &self,
        state: &mut RealRepoState,
    ) -> Result<(), RepoStateError> {
        let execution_ids = self
            .standalone_executions()?
            .into_keys()
            .collect::<BTreeSet<_>>();
        let projection_ids = self
            .standalone_projections()?
            .into_keys()
            .collect::<BTreeSet<_>>();
        state
            .executions
            .retain(|execution| !execution_ids.contains(&execution.execution_id));
        state
            .projections
            .retain(|projection| !projection_ids.contains(&projection.projection_id));
        Ok(())
    }

    pub(crate) fn recover(&self, state: &RealRepoState) -> Result<(), RepoStateError> {
        self.recover_orphan_projections()?;
        let running = self
            .standalone_executions()?
            .into_values()
            .filter(|stored| stored.snapshot.status == "running")
            .map(|stored| {
                (
                    stored.snapshot.execution_id.clone(),
                    stored.runner_instance_token,
                )
            })
            .collect::<Vec<_>>();
        for (execution_id, observed_token) in running {
            let Some(_lock) = self.try_acquire_lock(&execution_id)? else {
                continue;
            };
            let Some(stored) = self.read_stored_execution(&execution_id)? else {
                continue;
            };
            if stored.snapshot.status != "running" || stored.runner_instance_token != observed_token
            {
                continue;
            }
            let mut interrupted = stored.snapshot;
            interrupted.status = "interrupted".to_string();
            interrupted.runner_process_id = None;
            interrupted.command_outcome = "unknown".to_string();
            interrupted.termination_reason = Some("runner_process_terminated".to_string());
            interrupted.wait_failed = true;
            interrupted.finished_at = real_now_id();
            let _ = self.finalize(state, &interrupted, &stored.runner_instance_token, true)?;
        }
        Ok(())
    }

    fn recover_orphan_projections(&self) -> Result<(), RepoStateError> {
        let executions = self.standalone_executions()?;
        for stored in self.standalone_projections()?.into_values() {
            if executions.contains_key(&stored.execution_id) {
                continue;
            }
            let Some(_lock) = self.try_acquire_lock(&stored.execution_id)? else {
                continue;
            };
            if self.read_stored_execution(&stored.execution_id)?.is_some() {
                continue;
            }
            let path = self.projection_path(&stored.snapshot.projection_id);
            match fs::remove_file(&path) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(io_error(
                        &path,
                        "failed to remove orphan execution projection record",
                        error,
                    ))
                }
            }
            if let Some(root) = stored.snapshot.materialized_root.as_deref() {
                let root = Path::new(root);
                if let Some(allocation_root) = root.parent() {
                    match fs::remove_dir_all(allocation_root) {
                        Ok(()) => {}
                        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                        Err(error) => {
                            return Err(io_error(
                                allocation_root,
                                "failed to remove orphan execution projection",
                                error,
                            ))
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn standalone_executions(&self) -> Result<BTreeMap<String, StoredExecution>, RepoStateError> {
        let mut records = BTreeMap::new();
        for path in json_files(&self.executions_root())? {
            if let Some(stored) = self.read_stored_execution_path(&path)? {
                records.insert(stored.snapshot.execution_id.clone(), stored);
            }
        }
        Ok(records)
    }

    fn standalone_projections(&self) -> Result<BTreeMap<String, StoredProjection>, RepoStateError> {
        let mut records = BTreeMap::new();
        for path in json_files(&self.projections_root())? {
            if let Some(stored) = self.read_stored_projection_path(&path)? {
                records.insert(stored.snapshot.projection_id.clone(), stored);
            }
        }
        Ok(records)
    }

    fn read_stored_execution(
        &self,
        execution_id: &str,
    ) -> Result<Option<StoredExecution>, RepoStateError> {
        self.read_stored_execution_path(&self.execution_path(execution_id))
    }

    fn read_stored_execution_path(
        &self,
        path: &Path,
    ) -> Result<Option<StoredExecution>, RepoStateError> {
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(io_error(path, "failed to read execution record", error)),
        };
        let value = parse_json_record(&bytes).map_err(|error| {
            invalid_state(
                path.to_path_buf(),
                format!("execution record is invalid JSON: {error}"),
            )
        })?;
        let JsonValue::Object(object) = &value else {
            return Err(invalid_state(
                path.to_path_buf(),
                "execution record must be a JSON object",
            ));
        };
        let Some(snapshot) = object.get("snapshot") else {
            return Ok(None);
        };
        let token = object
            .get("runner_instance_token")
            .and_then(json_string)
            .ok_or_else(|| {
                invalid_state(
                    path.to_path_buf(),
                    "standalone execution record has no runner instance token",
                )
            })?
            .to_string();
        let snapshot = parse_real_execution_snapshot_value(snapshot, path)?;
        if path.file_stem().and_then(|name| name.to_str()) != Some(snapshot.execution_id.as_str()) {
            return Err(invalid_state(
                path.to_path_buf(),
                "execution record path does not match its identity",
            ));
        }
        Ok(Some(StoredExecution {
            snapshot,
            runner_instance_token: token,
        }))
    }

    fn read_stored_projection(
        &self,
        projection_id: &str,
    ) -> Result<Option<StoredProjection>, RepoStateError> {
        self.read_stored_projection_path(&self.projection_path(projection_id))
    }

    fn read_stored_projection_path(
        &self,
        path: &Path,
    ) -> Result<Option<StoredProjection>, RepoStateError> {
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(io_error(
                    path,
                    "failed to read execution projection record",
                    error,
                ))
            }
        };
        let value = parse_json_record(&bytes).map_err(|error| {
            invalid_state(
                path.to_path_buf(),
                format!("execution projection record is invalid JSON: {error}"),
            )
        })?;
        let JsonValue::Object(object) = &value else {
            return Err(invalid_state(
                path.to_path_buf(),
                "execution projection record must be a JSON object",
            ));
        };
        let Some(snapshot) = object.get("snapshot") else {
            return Ok(None);
        };
        let execution_id = object
            .get("execution_id")
            .and_then(json_string)
            .ok_or_else(|| {
                invalid_state(
                    path.to_path_buf(),
                    "standalone execution projection has no execution identity",
                )
            })?
            .to_string();
        let snapshot = parse_real_projection_snapshot_value(&self.repo_root, snapshot, path)?;
        if path.file_stem().and_then(|name| name.to_str()) != Some(snapshot.projection_id.as_str())
        {
            return Err(invalid_state(
                path.to_path_buf(),
                "execution projection record path does not match its identity",
            ));
        }
        Ok(Some(StoredProjection {
            execution_id,
            snapshot,
        }))
    }

    fn failpoint(&self, _name: &str) -> Result<(), RepoStateError> {
        #[cfg(debug_assertions)]
        if std::env::var(FAILPOINT_ENV).as_deref() == Ok(_name) {
            return Err(RepoStateError::Io {
                path: self.repo_root.clone(),
                message: format!("deterministic execution-store failpoint `{_name}`"),
            });
        }
        Ok(())
    }

    fn executions_root(&self) -> PathBuf {
        self.repo_root.join(".sunlight/executions")
    }

    fn projections_root(&self) -> PathBuf {
        self.repo_root.join(".sunlight/projections")
    }

    fn execution_path(&self, execution_id: &str) -> PathBuf {
        self.executions_root().join(format!("{execution_id}.json"))
    }

    fn projection_path(&self, projection_id: &str) -> PathBuf {
        self.projections_root()
            .join(format!("{projection_id}.json"))
    }

    fn reservation_root(&self) -> PathBuf {
        self.repo_root
            .join(".sunlight/local/execution-start-reservations")
    }

    fn lock_path(&self, execution_id: &str) -> PathBuf {
        self.repo_root
            .join(".sunlight/local/execution-locks")
            .join(format!("{execution_id}.lock"))
    }
}

fn execution_record_bytes(
    state: &RealRepoState,
    execution: &RealExecutionSnapshot,
    runner_instance_token: &str,
) -> Result<Vec<u8>, RepoStateError> {
    let display = real_execution_snapshot_record_json(state, execution);
    let mut value =
        parse_json_record(display.as_bytes()).map_err(|error| RepoStateError::InvalidState {
            path: PathBuf::from(".sunlight/executions"),
            message: format!("failed to construct standalone execution record: {error}"),
        })?;
    let JsonValue::Object(object) = &mut value else {
        unreachable!("execution display record is an object")
    };
    object.insert(
        "runner_instance_token".to_string(),
        JsonValue::String(runner_instance_token.to_string()),
    );
    object.insert(
        "snapshot".to_string(),
        real_execution_snapshot_json_value(execution),
    );
    canonical_json_bytes(&value).map_err(RepoStateError::from)
}

fn projection_record_bytes(
    state: &RealRepoState,
    execution_id: &str,
    projection: &RealProjectionSnapshot,
) -> Result<Vec<u8>, RepoStateError> {
    let display = real_projection_snapshot_json(state, projection);
    let mut value =
        parse_json_record(display.as_bytes()).map_err(|error| RepoStateError::InvalidState {
            path: PathBuf::from(".sunlight/projections"),
            message: format!("failed to construct standalone execution projection record: {error}"),
        })?;
    let JsonValue::Object(object) = &mut value else {
        unreachable!("projection display record is an object")
    };
    object.insert(
        "execution_id".to_string(),
        JsonValue::String(execution_id.to_string()),
    );
    object.insert(
        "snapshot".to_string(),
        real_projection_snapshot_json_value(projection),
    );
    canonical_json_bytes(&value).map_err(RepoStateError::from)
}

fn json_files(root: &Path) -> Result<Vec<PathBuf>, RepoStateError> {
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(io_error(root, "failed to list execution records", error)),
    };
    let mut paths = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|extension| extension.to_str()) == Some("json"))
        .collect::<Vec<_>>();
    paths.sort();
    Ok(paths)
}

fn json_string(value: &JsonValue) -> Option<&str> {
    match value {
        JsonValue::String(value) => Some(value),
        _ => None,
    }
}

fn invalid_state(path: PathBuf, message: impl Into<String>) -> RepoStateError {
    RepoStateError::InvalidState {
        path,
        message: message.into(),
    }
}

fn io_error(path: &Path, message: &str, error: io::Error) -> RepoStateError {
    RepoStateError::Io {
        path: path.to_path_buf(),
        message: format!("{message}: {error}"),
    }
}
