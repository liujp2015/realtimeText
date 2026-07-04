use sqlx::SqlitePool;
use std::sync::Arc;
use std::thread::JoinHandle;
use parking_lot::Mutex;
use tokio::sync::mpsc;
use tokio::task::JoinHandle as TokioJoinHandle;

use crate::config::AppConfig;

pub struct RunningHandle {
    pub session_guid: String,
    pub stop_tx: mpsc::Sender<()>,
    pub pipeline_tasks: Vec<TokioJoinHandle<()>>,
    pub capture_stop: std::sync::mpsc::Sender<()>,
    pub capture_thread: Option<JoinHandle<()>>,
}

pub struct AppState {
    pub pool: SqlitePool,
    pub config: Arc<Mutex<AppConfig>>,
    pub running: Arc<Mutex<Option<RunningHandle>>>,
}

impl AppState {
    pub fn new(pool: SqlitePool, config: AppConfig) -> Self {
        Self {
            pool,
            config: Arc::new(Mutex::new(config)),
            running: Arc::new(Mutex::new(None)),
        }
    }
}
