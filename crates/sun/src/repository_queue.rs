use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

const REPOSITORY_MUTATION_QUEUE_TIMEOUT: Duration = Duration::from_secs(10);
const REPOSITORY_MUTATION_QUEUE_POLL: Duration = Duration::from_millis(10);

#[derive(Debug)]
pub(crate) struct RepositoryMutationQueueGuard {
    _file: fs::File,
}

#[derive(Debug)]
pub(crate) enum RepositoryMutationQueueError {
    Cancelled { lock: PathBuf },
    Timeout { lock: PathBuf, timeout: Duration },
    Io { lock: PathBuf, message: String },
}

pub(crate) fn acquire_repository_mutation_queue(
    repo: &Path,
    cancel: Option<&AtomicBool>,
) -> Result<RepositoryMutationQueueGuard, RepositoryMutationQueueError> {
    let lock_path = repo.join(".sunlight/local/mcp-mutation-queue.lock");
    let parent = lock_path.parent().expect("queue lock has a parent");
    fs::create_dir_all(parent).map_err(|error| RepositoryMutationQueueError::Io {
        lock: lock_path.clone(),
        message: format!("cannot create the repository mutation queue directory: {error}"),
    })?;
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(&lock_path)
        .map_err(|error| RepositoryMutationQueueError::Io {
            lock: lock_path.clone(),
            message: format!("cannot open the repository mutation queue: {error}"),
        })?;
    let started = Instant::now();
    loop {
        if cancel.is_some_and(|cancel| cancel.load(Ordering::Acquire)) {
            return Err(RepositoryMutationQueueError::Cancelled { lock: lock_path });
        }
        match file.try_lock() {
            Ok(()) => return Ok(RepositoryMutationQueueGuard { _file: file }),
            Err(fs::TryLockError::WouldBlock) => {
                if started.elapsed() >= REPOSITORY_MUTATION_QUEUE_TIMEOUT {
                    return Err(RepositoryMutationQueueError::Timeout {
                        lock: lock_path,
                        timeout: REPOSITORY_MUTATION_QUEUE_TIMEOUT,
                    });
                }
                thread::sleep(REPOSITORY_MUTATION_QUEUE_POLL);
            }
            Err(fs::TryLockError::Error(error)) => {
                return Err(RepositoryMutationQueueError::Io {
                    lock: lock_path,
                    message: format!("cannot lock the repository mutation queue: {error}"),
                });
            }
        }
    }
}
