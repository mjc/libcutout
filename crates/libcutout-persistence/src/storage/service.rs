use super::{
    BootstrapSnapshot, COMMAND_QUEUE_CAPACITY, Command, RideDatabase, StorageError,
    canonical_database_path, configure_connection, worker,
};
use rusqlite::Connection;
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, SyncSender},
    },
    thread::{self, JoinHandle},
};
use uuid::Uuid;

struct OwnerEntry {
    path: PathBuf,
    service_id: Uuid,
    bootstrap: BootstrapSnapshot,
    sender: SyncSender<Command>,
    shutting_down: bool,
    worker_alive: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

static OWNER: OnceLock<Mutex<Option<OwnerEntry>>> = OnceLock::new();

fn owner() -> &'static Mutex<Option<OwnerEntry>> {
    OWNER.get_or_init(|| Mutex::new(None))
}

pub(super) fn open(path: &Path) -> Result<RideDatabase, StorageError> {
    acquire(path, Uuid::new_v4())
}

pub(super) fn reopen(path: &Path, service_id: Uuid) -> Result<RideDatabase, StorageError> {
    acquire(path, service_id)
}

fn acquire(path: &Path, service_id: Uuid) -> Result<RideDatabase, StorageError> {
    let canonical_path = canonical_database_path(path)?;
    let mut owner = owner().lock().map_err(|_| StorageError::WorkerStopped)?;
    remove_stale_owner(&mut owner);

    if let Some(existing) = owner.as_ref() {
        if existing.path != canonical_path {
            return Err(StorageError::AlreadyOpenForDifferentPath);
        }
        return Ok(handle_from(existing));
    }

    let mut connection = Connection::open(&canonical_path)?;
    let bootstrap = configure_connection(&mut connection)?;
    let (sender, receiver) = mpsc::sync_channel(COMMAND_QUEUE_CAPACITY);
    let worker_alive = Arc::new(AtomicBool::new(true));
    let worker_alive_for_thread = Arc::clone(&worker_alive);
    let join = thread::Builder::new()
        .name("cutout-ride-maps-db".to_owned())
        .spawn(move || worker::run(connection, &receiver, &worker_alive_for_thread))
        .map_err(|error| StorageError::WorkerStart(error.to_string()))?;
    let handle = RideDatabase {
        sender: sender.clone(),
        service_id,
        bootstrap: bootstrap.clone(),
        path: Arc::new(canonical_path.clone()),
    };
    *owner = Some(OwnerEntry {
        path: canonical_path,
        service_id,
        bootstrap,
        sender,
        worker_alive,
        shutting_down: false,
        join: Some(join),
    });
    Ok(handle)
}

pub(super) fn begin_shutdown(service_id: Uuid) -> Result<SyncSender<Command>, StorageError> {
    let mut owner = owner().lock().map_err(|_| StorageError::WorkerStopped)?;
    let Some(entry) = owner.as_mut() else {
        return Err(StorageError::WorkerStopped);
    };
    if entry.service_id != service_id || entry.shutting_down {
        return Err(StorageError::WorkerStopped);
    }
    entry.shutting_down = true;
    Ok(entry.sender.clone())
}

pub(super) fn cancel_shutdown(service_id: Uuid) {
    if let Ok(mut owner) = owner().lock()
        && let Some(entry) = owner.as_mut()
        && entry.service_id == service_id
    {
        entry.shutting_down = false;
    }
}

pub(super) fn finish_shutdown(service_id: Uuid) -> Result<(), StorageError> {
    let mut owner = owner().lock().map_err(|_| StorageError::WorkerStopped)?;
    let Some(mut existing) = owner.take() else {
        return Ok(());
    };
    if existing.service_id != service_id {
        *owner = Some(existing);
        return Err(StorageError::WorkerStopped);
    }
    if let Some(join) = existing.join.take() {
        join.join().map_err(|_| StorageError::WorkerStopped)?;
    }
    Ok(())
}

pub(super) fn worker_has_exited(service_id: Uuid) -> bool {
    owner().lock().ok().is_some_and(|owner| {
        owner.as_ref().is_some_and(|entry| {
            entry.service_id == service_id && !entry.worker_alive.load(Ordering::Acquire)
        })
    })
}

pub(super) fn can_restart(service_id: Uuid) -> bool {
    owner().lock().ok().is_some_and(|owner| {
        owner
            .as_ref()
            .is_some_and(|entry| entry.service_id == service_id && !entry.shutting_down)
    })
}

#[cfg(test)]
pub(super) fn wait_for_worker_exit(service_id: Uuid) {
    loop {
        let finished = owner().lock().ok().and_then(|owner| {
            let entry = owner.as_ref()?;
            (entry.service_id == service_id)
                .then(|| entry.join.as_ref().map(JoinHandle::is_finished))
                .flatten()
        });
        if finished != Some(false) {
            return;
        }
        thread::yield_now();
    }
}

fn remove_stale_owner(owner: &mut Option<OwnerEntry>) {
    let is_stale = owner.as_ref().is_some_and(|entry| {
        entry.join.as_ref().is_some_and(JoinHandle::is_finished)
            || !entry.worker_alive.load(Ordering::Acquire)
    });
    if is_stale
        && let Some(mut stale) = owner.take()
        && let Some(join) = stale.join.take()
    {
        let _ = join.join();
    }
}

fn handle_from(owner: &OwnerEntry) -> RideDatabase {
    RideDatabase {
        sender: owner.sender.clone(),
        service_id: owner.service_id,
        bootstrap: owner.bootstrap.clone(),
        path: Arc::new(owner.path.clone()),
    }
}
