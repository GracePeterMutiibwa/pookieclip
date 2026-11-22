use std::path::PathBuf;

use tauri::{AppHandle, State};

use crate::clip_writer;
use crate::file_store::{self, FileTree};
use crate::store::{Clip, Store};

#[tauri::command]
pub fn list_clips(store: State<'_, Store>) -> Result<Vec<Clip>, String> {
    store.list().map_err(|error| error.to_string())
}

#[tauri::command]
pub fn set_pinned(store: State<'_, Store>, id: i64, pinned: bool) -> Result<(), String> {
    store.set_pinned(id, pinned).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn remove_clip(store: State<'_, Store>, id: i64) -> Result<(), String> {
    store.remove(id).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn clear_history(store: State<'_, Store>) -> Result<(), String> {
    store.clear_unpinned().map_err(|error| error.to_string())
}

#[tauri::command]
pub fn restore_clips(store: State<'_, Store>, clips: Vec<Clip>) -> Result<(), String> {
    store.restore(&clips).map_err(|error| error.to_string())
}

// GTK clipboard calls only work on the main thread, so hand the write over to it
#[tauri::command]
pub fn copy_clip(app: AppHandle, mime_type: String, content: String) -> Result<(), String> {
    app.run_on_main_thread(move || clip_writer::write(&mime_type, &content))
        .map_err(|error| error.to_string())
}

// The files of one entry as a tree, for the panel's file view
#[tauri::command]
pub fn file_tree(store: State<'_, Store>, id: i64) -> Result<FileTree, String> {
    let clip = store
        .get(id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "entry not found".to_string())?;
    Ok(file_store::tree_for(&clip.content, clip.stored.as_deref()))
}

// Copies chosen files from an entry's tree, given as full paths
#[tauri::command]
pub fn copy_paths(app: AppHandle, paths: Vec<String>) -> Result<(), String> {
    let paths: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
    let uris = file_store::uris_from_paths(&paths);
    app.run_on_main_thread(move || clip_writer::write("text/uri-list", &uris))
        .map_err(|error| error.to_string())
}
