mod clip_bus;
mod clip_writer;
mod commands;
mod first_run;
mod file_store;
mod store;
use tauri::{AppHandle, Emitter, Manager, WindowEvent};

const PANEL_LABEL: &str = "panel";

fn show_panel(app: &AppHandle) {
    let Some(window) = app.get_webview_window(PANEL_LABEL) else {
        return;
    };
    let _ = window.center();
    let _ = window.show();
    let _ = window.set_focus();
    let _ = window.emit("panel-shown", ());
}

fn hide_panel(app: &AppHandle) {
    let Some(window) = app.get_webview_window(PANEL_LABEL) else {
        return;
    };
    if window.is_visible().unwrap_or(false) {
        let _ = window.hide();
        // Copies of removed entries are deleted here, after the undo window has closed
        file_store::collect_garbage(app);
    }
}

fn toggle_panel(app: &AppHandle) {
    let Some(window) = app.get_webview_window(PANEL_LABEL) else {
        return;
    };
    if window.is_visible().unwrap_or(false) {
        hide_panel(app);
    } else {
        show_panel(app);
    }
}

#[tauri::command]
fn close_panel(app: AppHandle) {
    hide_panel(&app);
}

#[tauri::command]
fn quit_app(app: AppHandle) {
    app.exit(0);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // A second launch talks to this running instance instead of starting a new one
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            if args.iter().any(|arg| arg == "--toggle") {
                toggle_panel(app);
            } else {
                show_panel(app);
            }
        }))
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            close_panel,
            quit_app,
            commands::list_clips,
            commands::set_pinned,
            commands::remove_clip,
            commands::clear_history,
            commands::restore_clips,
            commands::copy_clip,
            commands::file_tree,
            commands::copy_paths
        ])
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                hide_panel(window.app_handle());
            }
        })
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }


            // History lives in the app's data folder, created on first run
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            app.manage(store::Store::open(&data_dir.join("history.db"))?);
            first_run::run_once(app.handle(), &data_dir);

            // PookieClip's own copies of copied files live next to the history
            let files_dir = data_dir.join("files");
            std::fs::create_dir_all(&files_dir)?;
            app.manage(file_store::FileStore::new(files_dir));
            file_store::clean_at_startup(app.handle());

            // The connection lives in app state so the listener stays open
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                match clip_bus::start(handle.clone()).await {
                    Ok(connection) => {
                        handle.manage(connection);
                    }
                    Err(error) => log::error!("could not start the clipboard listener: {error}"),
                }
            });

            // Autostart passes --hidden so the panel stays out of the way at login
            if !std::env::args().any(|arg| arg == "--hidden") {
                show_panel(app.handle());
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while building tauri application");
}