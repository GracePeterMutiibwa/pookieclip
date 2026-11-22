use std::collections::HashSet;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use gtk::gio::File;
use gtk::prelude::*;
use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, Manager};

use crate::store::Store;

// All stored copies share this budget, the oldest unpinned ones are dropped first
const MAX_STORED_BYTES: i64 = 10 * 1024 * 1024 * 1024;

// Huge folders are cut off in the panel so the tree stays quick to show
const MAX_TREE_NODES: usize = 5000;

const STAGING_PREFIX: &str = ".staging-";

// PookieClip keeps its own copy of every copied file as files/<hash>/<original name>
pub struct FileStore {
    pub dir: PathBuf,
    // Held while copying, so cleanup never deletes a copy that isn't recorded yet
    busy: Mutex<()>,
}

impl FileStore {
    pub fn new(dir: PathBuf) -> FileStore {
        FileStore {
            dir,
            busy: Mutex::new(()),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TreeNode {
    name: String,
    path: String,
    is_dir: bool,
    size: u64,
    children: Vec<TreeNode>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileTree {
    origin: String,
    roots: Vec<TreeNode>,
    stored: bool,
    truncated: bool,
}

pub fn paths_from_uris(uris: &str) -> Vec<PathBuf> {
    uris.lines()
        .filter(|uri| uri.starts_with("file://"))
        .filter_map(|uri| File::for_uri(uri).path())
        .collect()
}

pub fn uris_from_paths(paths: &[PathBuf]) -> String {
    paths
        .iter()
        .map(|path| File::for_path(path).uri().to_string())
        .collect::<Vec<_>>()
        .join("\n")
}

// Size of a file or a whole folder, without following symlinks so loops can't happen
fn size_of(path: &Path) -> u64 {
    let Ok(meta) = fs::symlink_metadata(path) else {
        return 0;
    };
    if meta.is_dir() {
        fs::read_dir(path)
            .map(|entries| entries.flatten().map(|entry| size_of(&entry.path())).sum())
            .unwrap_or(0)
    } else if meta.is_file() {
        meta.len()
    } else {
        0
    }
}

// Copies a file or folder to target, feeding names and contents into the hash as it goes
fn copy_hashing(source: &Path, target: &Path, hasher: &mut Sha256) -> std::io::Result<()> {
    let meta = fs::symlink_metadata(source)?;
    if meta.is_dir() {
        fs::create_dir_all(target)?;
        hasher.update(b"dir");
        let mut entries: Vec<_> = fs::read_dir(source)?.flatten().collect();
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            hasher.update(entry.file_name().as_encoded_bytes());
            copy_hashing(&entry.path(), &target.join(entry.file_name()), hasher)?;
        }
    } else if meta.is_file() {
        hasher.update(b"file");
        let mut reader = fs::File::open(source)?;
        let mut writer = fs::File::create(target)?;
        let mut buffer = vec![0u8; 1024 * 1024];
        loop {
            let read = reader.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            hasher.update(&buffer[..read]);
            writer.write_all(&buffer[..read])?;
        }
    }
    Ok(())
}

// Saves one copied file or folder and returns where its copy lives.
// The same name and contents always land in the same folder, so repeats take no extra space.
fn store_one(files_dir: &Path, source: &Path) -> std::io::Result<PathBuf> {
    let name = source
        .file_name()
        .ok_or_else(|| std::io::Error::other("copied path has no file name"))?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let staging = files_dir.join(format!("{STAGING_PREFIX}{stamp}"));
    fs::create_dir_all(&staging)?;

    let mut hasher = Sha256::new();
    hasher.update(name.as_encoded_bytes());
    if let Err(error) = copy_hashing(source, &staging.join(name), &mut hasher) {
        let _ = fs::remove_dir_all(&staging);
        return Err(error);
    }

    let key: String = hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let final_dir = files_dir.join(key);
    if final_dir.exists() {
        fs::remove_dir_all(&staging)?;
    } else {
        fs::rename(&staging, &final_dir)?;
    }
    Ok(final_dir.join(name))
}

// Deletes stored copies that no entry points at anymore. Leftovers from interrupted copies
// are only removed at startup, because during a run they may belong to a copy in progress.
fn remove_unreferenced(app: &AppHandle, files_dir: &Path, include_staging: bool) {
    let Ok(lists) = app.state::<Store>().stored_lists() else {
        return;
    };
    let keep: HashSet<String> = lists
        .iter()
        .flat_map(|list| paths_from_uris(list))
        .filter_map(|path| {
            path.strip_prefix(files_dir)
                .ok()
                .and_then(|relative| relative.components().next())
                .map(|key| key.as_os_str().to_string_lossy().into_owned())
        })
        .collect();
    let Ok(entries) = fs::read_dir(files_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with(STAGING_PREFIX) && !include_staging {
            continue;
        }
        if !keep.contains(&name) {
            let _ = fs::remove_dir_all(entry.path());
        }
    }
}

pub fn clean_at_startup(app: &AppHandle) {
    let file_store = app.state::<FileStore>();
    remove_unreferenced(app, &file_store.dir, true);
}

// Runs when the panel hides. Skipped while a copy is in progress, the next hide catches up.
pub fn collect_garbage(app: &AppHandle) {
    let file_store = app.state::<FileStore>();
    let Ok(_guard) = file_store.busy.try_lock() else {
        return;
    };
    remove_unreferenced(app, &file_store.dir, false);
}

// Saves PookieClip's own copies of an entry's files in the background, so a cut and paste
// or a later delete of the originals doesn't stop them from being pasted again
pub fn store_copies(app: AppHandle, clip_id: i64, uris: String) {
    std::thread::spawn(move || {
        let file_store = app.state::<FileStore>();
        let _guard = file_store.busy.lock().unwrap();
        let store = app.state::<Store>();

        let sources: Vec<PathBuf> = paths_from_uris(&uris)
            .into_iter()
            .filter(|path| path.exists())
            .collect();
        if sources.is_empty() {
            return;
        }

        // A copy bigger than the whole budget keeps only its original locations
        let needed: i64 = sources.iter().map(|path| size_of(path) as i64).sum();
        if needed > MAX_STORED_BYTES {
            log::info!("skipped storing a {needed} byte copy, over the storage limit");
            return;
        }
        if let Err(error) = store.make_room(MAX_STORED_BYTES - needed) {
            log::error!("could not make room for stored copies: {error}");
            return;
        }
        remove_unreferenced(&app, &file_store.dir, false);

        let mut stored = Vec::new();
        for source in &sources {
            match store_one(&file_store.dir, source) {
                Ok(path) => stored.push(path),
                Err(error) => {
                    // Copies saved so far are unreferenced and get cleaned up later
                    log::error!("could not store a copy of {}: {error}", source.display());
                    return;
                }
            }
        }

        match store.set_stored(clip_id, &uris_from_paths(&stored), needed) {
            Ok(Some(clip)) => {
                let _ = app.emit("clip-updated", clip);
            }
            Ok(None) => {}
            Err(error) => log::error!("could not record stored copies: {error}"),
        }
    });
}

// How many more nodes the tree may hold, and whether anything was left out
struct Budget {
    left: usize,
    truncated: bool,
}

fn build_node(path: &Path, budget: &mut Budget) -> Option<TreeNode> {
    if budget.left == 0 {
        budget.truncated = true;
        return None;
    }
    let meta = fs::symlink_metadata(path).ok()?;
    let name = path.file_name()?.to_string_lossy().into_owned();
    let path_text = path.to_string_lossy().into_owned();
    budget.left -= 1;
    if meta.is_dir() {
        let mut entries: Vec<_> = fs::read_dir(path).ok()?.flatten().map(|entry| entry.path()).collect();
        entries.sort();
        let mut children: Vec<TreeNode> = entries
            .iter()
            .filter_map(|child| build_node(child, budget))
            .collect();
        // Folders first, then names in a case insensitive order
        children.sort_by(|a, b| {
            b.is_dir
                .cmp(&a.is_dir)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        let size = children.iter().map(|child| child.size).sum();
        Some(TreeNode {
            name,
            path: path_text,
            is_dir: true,
            size,
            children,
        })
    } else if meta.is_file() {
        Some(TreeNode {
            name,
            path: path_text,
            is_dir: false,
            size: meta.len(),
            children: Vec::new(),
        })
    } else {
        None
    }
}

// The folder the copied items came from, shown with ~ for the home folder
fn origin_of(originals: &[PathBuf]) -> String {
    let mut common: Option<PathBuf> = None;
    for path in originals {
        let parent = path.parent().map(Path::to_path_buf).unwrap_or_default();
        common = Some(match common {
            None => parent,
            Some(current) => current
                .components()
                .zip(parent.components())
                .take_while(|(a, b)| a == b)
                .map(|(a, _)| a)
                .collect(),
        });
    }
    let origin = common.unwrap_or_default().to_string_lossy().into_owned();
    match std::env::var("HOME") {
        Ok(home) if !home.is_empty() && origin.starts_with(&home) => format!("~{}", &origin[home.len()..]),
        _ => origin,
    }
}

// The tree shown for a file entry, read from the stored copies when they exist,
// otherwise from the original locations that still exist
pub fn tree_for(content: &str, stored: Option<&str>) -> FileTree {
    let originals = paths_from_uris(content);
    let (paths, is_stored) = match stored {
        Some(list) => (paths_from_uris(list), true),
        None => (originals.clone(), false),
    };
    let mut budget = Budget {
        left: MAX_TREE_NODES,
        truncated: false,
    };
    let roots: Vec<TreeNode> = paths.iter().filter_map(|path| build_node(path, &mut budget)).collect();
    FileTree {
        origin: origin_of(&originals),
        roots,
        stored: is_stored,
        truncated: budget.truncated,
    }
}
