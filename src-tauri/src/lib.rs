pub mod audio;
pub mod commands;
pub mod context;
pub mod hotkey;
pub mod inject;
pub mod models;
pub mod pipeline;
pub mod prompt;
pub mod providers;
pub mod store;

use std::sync::{Arc, Mutex};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::Manager;

pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "localflow=debug,info".into()),
        )
        .init();

    tauri::Builder::default()
        // MUST be the first plugin. A second launch (e.g. the installed app
        // while `tauri dev` runs) otherwise spins up a rival global-hotkey +
        // clipboard handler — the two race on the pasteboard and the paste
        // lands the *old* clipboard. Reject the second instance instead.
        .plugin(tauri_plugin_single_instance::init(|_app, _args, _cwd| {}))
        .plugin(tauri_plugin_macos_permissions::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .invoke_handler(tauri::generate_handler![
            commands::get_status,
            commands::get_settings,
            commands::set_setting,
            commands::set_api_key,
            commands::list_models,
            commands::download_model,
            commands::delete_model,
            commands::history_list,
            commands::history_delete,
            commands::history_clear,
            commands::dict_list,
            commands::dict_add,
            commands::dict_delete,
            commands::snippets_list,
            commands::snippet_add,
            commands::snippet_delete,
            commands::styles_list,
            commands::style_update,
            commands::ollama_status,
            commands::transcribe_wav
        ])
        .setup(|app| {
            // Tray-only app: no dock icon (plan 0.1).
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let db_path = app
                .path()
                .app_data_dir()
                .expect("app data dir")
                .join("localflow.db");
            let store = Arc::new(store::Store::open(&db_path).expect("opening database"));

            app.manage(commands::AppState {
                status: Mutex::new(commands::AppStatus::default()),
                store: store.clone(),
                pipeline: Mutex::new(None),
            });

            let settings =
                MenuItem::with_id(app, "settings", "Settings…", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit LocalFlow", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&settings, &quit])?;
            // Dedicated template glyph (black + alpha) — the colorful app icon
            // renders as a solid blob in template mode.
            TrayIconBuilder::with_id("main")
                .icon(tauri::image::Image::from_bytes(include_bytes!(
                    "../icons/tray.png"
                ))?)
                .icon_as_template(true)
                .menu(&menu)
                .show_menu_on_left_click(true)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "settings" => {
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .build(app)?;

            // Pill overlay: click-through and ALWAYS visible — macOS suspends
            // WKWebViews in hidden windows, so show/hide at the window level
            // silently breaks event delivery. Visibility lives in the DOM
            // (transparent window renders nothing when the pill is idle).
            if let Some(pill) = app.get_webview_window("pill") {
                let _ = pill.set_ignore_cursor_events(true);
                pipeline::position_pill(app.handle(), &pill);
                let _ = pill.show();
                // Windows created hidden never lay out their WKWebView — the
                // surface stays fully transparent (verified by window-ID
                // screenshot: max alpha 0 with static HTML loaded). A real
                // resize forces the layout pass.
                if let Ok(sz) = pill.outer_size() {
                    let _ = pill.set_size(tauri::PhysicalSize::new(sz.width, sz.height + 2));
                    let _ = pill.set_size(sz);
                }
                // Above the Dock (level 20) so auto-hide can't cover the pill.
                #[cfg(target_os = "macos")]
                if let Ok(ptr) = pill.ns_window() {
                    let ns: &objc2_app_kit::NSWindow =
                        unsafe { &*ptr.cast::<objc2_app_kit::NSWindow>() };
                    unsafe { ns.setLevel(21) };
                }
            }

            let handle = pipeline::spawn(app.handle().clone(), store);
            let state: tauri::State<commands::AppState> = app.state();
            *state.pipeline.lock().expect("pipeline poisoned") = Some(handle);
            Ok(())
        })
        .on_page_load(|window, payload| {
            tracing::info!(
                "page load: window={} url={} event={:?}",
                window.label(),
                payload.url(),
                payload.event()
            );
        })
        .on_window_event(|window, event| {
            // Closing the settings window hides it; the app lives in the tray.
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let _ = window.hide();
                api.prevent_close();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running LocalFlow");
}
