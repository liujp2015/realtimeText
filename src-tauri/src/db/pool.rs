use anyhow::{Context, Result};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::{path::Path, str::FromStr};
use tauri::{AppHandle, Manager};

pub async fn init(app: &AppHandle) -> Result<SqlitePool> {
    let data_dir = app
        .path()
        .app_data_dir()
        .context("failed to resolve app_data_dir")?;
    std::fs::create_dir_all(&data_dir)
        .with_context(|| format!("create app_data_dir {:?}", data_dir))?;

    let db_path = data_dir.join("subtitle.db");
    let db_url = format!(
        "sqlite://{}?mode=rwc",
        db_path
            .to_str()
            .context("non-utf8 db path")?
            .replace('\\', "/")
    );

    let options = SqliteConnectOptions::from_str(&db_url)?
        .create_if_missing(true)
        .foreign_keys(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await
        .context("connect sqlite pool")?;

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .context("run migrations")?;

    log::info!("sqlite pool ready at {:?}", db_path);
    let _ = Path::new(&db_path);
    Ok(pool)
}
