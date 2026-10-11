//! Updates: what the last check found, a check now, and installing it.

use tauri::AppHandle;

use crate::update::UpdateStatus;

#[tauri::command]
#[specta::specta]
pub async fn update_status(app: AppHandle) -> Result<UpdateStatus, ()> {
    Ok(crate::update::status(&app))
}

/// Check now, whatever the setting says: the reader asked.
#[tauri::command]
#[specta::specta]
pub async fn update_check(app: AppHandle) -> Result<UpdateStatus, ()> {
    Ok(crate::update::check(&app).await)
}

/// Install what the last check found. Lurkn restarts when it is done.
#[tauri::command]
#[specta::specta]
pub async fn update_install(app: AppHandle) -> Result<(), String> {
    crate::update::install(&app).await
}
