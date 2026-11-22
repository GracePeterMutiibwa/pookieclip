use gtk::gio::File;
use gtk::prelude::*;
use gtk::{gdk, Clipboard, TargetEntry, TargetFlags};

// Every format offered for a clip, so each app can paste the kind it understands
fn formats_for(mime_type: &str, content: &str) -> Vec<(String, Vec<u8>)> {
    if mime_type == "text/uri-list" {
        let uris: Vec<&str> = content
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .collect();
        let paths: Vec<String> = uris
            .iter()
            .filter_map(|uri| File::for_uri(uri).path())
            .map(|path| path.to_string_lossy().into_owned())
            .collect();
        let plain = paths.join("\n").into_bytes();
        return vec![
            ("x-special/gnome-copied-files".into(), format!("copy\n{}", uris.join("\n")).into_bytes()),
            ("text/uri-list".into(), format!("{}\r\n", uris.join("\r\n")).into_bytes()),
            ("text/plain;charset=utf-8".into(), plain.clone()),
            ("UTF8_STRING".into(), plain),
        ];
    }

    let bytes = content.as_bytes().to_vec();
    ["text/plain;charset=utf-8", "UTF8_STRING", "text/plain", "STRING"]
        .iter()
        .map(|format| (format.to_string(), bytes.clone()))
        .collect()
}

// Must run on the GTK main thread
pub fn write(mime_type: &str, content: &str) {
    let formats = formats_for(mime_type, content);
    let targets: Vec<TargetEntry> = formats
        .iter()
        .enumerate()
        .map(|(index, (format, _))| TargetEntry::new(format, TargetFlags::empty(), index as u32))
        .collect();
    let clipboard = Clipboard::get(&gdk::SELECTION_CLIPBOARD);
    clipboard.set_with_data(&targets, move |_clipboard, selection, index| {
        if let Some((_, data)) = formats.get(index as usize) {
            selection.set(&selection.target(), 8, data);
        }
    });
}