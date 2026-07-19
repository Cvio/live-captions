use crate::pipeline::{self, PipelineState};
use log::{info, warn};
use tauri::{AppHandle, Manager};

#[tauri::command]
#[specta::specta]
pub fn start_captions(app: AppHandle) -> Result<(), String> {
    let state = app.state::<PipelineState>();
    let mut running = state.0.lock().unwrap();

    if running.is_some() {
        warn!("start_captions called while the caption pipeline is already running");
        return Ok(());
    }

    let handle = pipeline::start(&app)?;
    *running = Some(handle);
    info!("Caption pipeline started");
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn stop_captions(app: AppHandle) -> Result<(), String> {
    let state = app.state::<PipelineState>();
    let handle = state.0.lock().unwrap().take();

    match handle {
        Some(handle) => pipeline::stop(handle),
        None => info!("stop_captions called while no caption pipeline is running"),
    }
    Ok(())
}
