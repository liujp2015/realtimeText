use tauri::State;

use crate::db::repository::{search_keywords as repo_search, SearchResultItem};
use crate::state::AppState;

#[tauri::command]
pub async fn search_keywords(
    keyword: String,
    state: State<'_, AppState>,
) -> Result<Vec<SearchResultItem>, String> {
    repo_search(&state.pool, &keyword)
        .await
        .map_err(|e| e.to_string())
}
