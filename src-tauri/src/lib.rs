mod clip_bus;
mod clip_writer;
mod commands;
mod store;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, WindowEvent};

const PANEL_LABEL: &str = "panel";

// Tray menu parts kept around so Open can be removed and restored
struct TrayMenu {
    menu: Menu<tauri::Wry>,
    open_item: MenuItem<tauri::Wry>,
}

fn show_panel(app: &AppHandle) {
    let Some(window) = app.get_webview_window(PANEL_LABEL) else {
        return;
    };
    if !window.is_visible().unwrap_or(false) {
        if let Some(tray_menu) = app.try_state::<TrayMenu>() {
            let _ = tray_menu.menu.remove(&tray_menu.open_item);
        }
    }
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
        if let Some(tray_menu) = app.try_state::<TrayMenu>() {
            let _ = tray_menu.menu.prepend(&tray_menu.open_item);
        }
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
            commands::list_clips,
            commands::set_pinned,
            commands::remove_clip,
            commands::clear_history,
            commands::restore_clips,
            commands::copy_clip,
            commands::remove_missing_files
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
            
            let open_item = MenuItem::with_id(app, "open", "Open PookieClip", true, None::<&str>)?;
            let about_item = MenuItem::with_id(app, "about", "About", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let tray_menu = Menu::with_items(app, &[&open_item, &about_item, &quit_item])?;

            TrayIconBuilder::with_id("main")
                .icon(app.default_window_icon().expect("app icon is bundled").clone())
                .tooltip("PookieClip")
                .menu(&tray_menu)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "open" => show_panel(app),
                    "about" => {
                        show_panel(app);
                        let _ = app.emit("show-about", ());
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .build(app)?;

                app.manage(TrayMenu {
                    menu: tray_menu,
                    open_item,
                });


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