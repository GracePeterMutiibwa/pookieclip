use gtk::gio::File;
use gtk::prelude::*;
use tauri::{AppHandle, Emitter, Manager};
use zbus::{connection, interface};

use crate::file_store::{self, FileStore};
use crate::store::Store;

const BUS_NAME: &str = "io.github.gracepetermutiibwa.PookieClip";
const OBJECT_PATH: &str = "/io/github/gracepetermutiibwa/PookieClip";


// Turns %20 style escapes back into the characters they stand for
fn decode_percent(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).ok();
            if let Some(value) = hex.and_then(|hex| u8::from_str_radix(hex, 16).ok()) {
                decoded.push(value);
                index += 3;
                continue;
            }
        }
        decoded.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

// One standard form for file references, so a copy, cut and paste of the same file merge.
// Plain text becomes a file reference only when it is the path of a file entry already in
// history, which is how file managers send the path again at paste time. Other text stays text.
fn normalize(mime_type: &str, content: &str, store: &Store) -> (String, String) {
    if mime_type == "text/uri-list" {
        let uris: Vec<String> = content
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .map(|uri| match uri.strip_prefix("file://") {
                Some(path) => File::for_path(decode_percent(path)).uri().to_string(),
                None => uri.to_string(),
            })
            .collect();
        return (mime_type.to_string(), uris.join("\n"));
    }
    // Several files arrive as one path per line, in the same order as their file list
    let lines: Vec<&str> = content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    if !lines.is_empty() && lines.iter().all(|line| line.starts_with('/')) {
        let uris = lines
            .iter()
            .map(|line| File::for_path(line).uri().to_string())
            .collect::<Vec<_>>()
            .join("\n");
        if matches!(store.find_id(&uris), Ok(Some(_))) {
            return ("text/uri-list".to_string(), uris);
        }
    }
    (mime_type.to_string(), content.to_string())
}

struct ClipBus {
    app: AppHandle,
}

// zbus turns add_clip into AddClip on the bus, matching the extension
#[interface(name = "io.github.gracepetermutiibwa.PookieClip")]
impl ClipBus {
    fn add_clip(&self, mime_type: String, source_app: String, data: Vec<u8>) {
        // Images come in their own step, text and file lists for now
        if mime_type.starts_with("image/") {
            return;
        }

        let raw = String::from_utf8_lossy(&data);
        if raw.trim().is_empty() {
            return;
        }

        let store = self.app.state::<Store>();
        let (mime_type, content) = normalize(&mime_type, &raw, &store);

        // Copies made from PookieClip's own stored files are re-copies of an entry, not new history
        if mime_type == "text/uri-list" {
            let files_dir = &self.app.state::<FileStore>().dir;
            let paths = file_store::paths_from_uris(&content);
            if !paths.is_empty() && paths.iter().all(|path| path.starts_with(files_dir)) {
                if let Ok(Some(clip)) = store.touch_stored(&content) {
                    let _ = self.app.emit("clip-added", clip);
                }
                return;
            }
        }

        match store.add(&mime_type, &content, &source_app) {
            Ok(clip) => {
                // New file entries get their own copies saved, repeats already have them
                if clip.mime_type == "text/uri-list" && clip.stored.is_none() {
                    file_store::store_copies(self.app.clone(), clip.id, clip.content.clone());
                }
                let _ = self.app.emit("clip-added", clip);
            }
            Err(error) => log::error!("could not save a clip: {error}"),
        }
    }

    fn add_skipped(&self) {
        let _ = self.app.emit("clip-skipped", ());
    }

    // Called when the GNOME extension's top bar icon is clicked
    fn toggle(&self) {
        crate::toggle_panel(&self.app);
    }
}

pub async fn start(app: AppHandle) -> zbus::Result<connection::Connection> {
    connection::Builder::session()?
        .name(BUS_NAME)?
        .serve_at(OBJECT_PATH, ClipBus { app })?
        .build()
        .await
}

