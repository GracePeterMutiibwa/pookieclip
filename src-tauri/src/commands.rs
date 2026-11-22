use gtk::gio::File;
use gtk::prelude::*;
use tauri::{AppHandle, State};

use crate::clip_writer;
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


// Deletes file entries whose files no longer exist and returns their ids
#[tauri::command]
pub fn remove_missing_files(store: State<'_, Store>) -> Result<Vec<i64>, String> {
    let clips = store.list().map_err(|error| error.to_string())?;
    let mut removed = Vec::new();
    for clip in clips.iter().filter(|clip| clip.mime_type == "text/uri-list") {
        let paths: Vec<_> = clip
            .content
            .lines()
            .filter(|uri| uri.starts_with("file://"))
            .filter_map(|uri| File::for_uri(uri).path())
            .collect();
        // Links in a uri list have no paths, so they are never treated as missing
        if !paths.is_empty() && !paths.iter().any(|path| path.exists()) {
            store.remove(clip.id).map_err(|error| error.to_string())?;
            removed.push(clip.id);
        }
    }
    Ok(removed)
}