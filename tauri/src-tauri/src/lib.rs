use tauri::Manager;

#[cfg(target_os = "macos")]
use window_vibrancy::{apply_vibrancy, NSVisualEffectMaterial, NSVisualEffectState};
#[cfg(target_os = "windows")]
use window_vibrancy::apply_mica;

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let window = app.get_webview_window("main").unwrap();

            // Sur Linux, aucun materiau natif : on evite le warning "unused".
            #[cfg(not(any(target_os = "macos", target_os = "windows")))]
            let _ = &window;

            // macOS : Vibrancy natif (NSVisualEffectView "Liquid Glass").
            // None pour theme/state laisse le systeme suivre l'apparence claire/sombre.
            #[cfg(target_os = "macos")]
            apply_vibrancy(
                &window,
                NSVisualEffectMaterial::UnderWindowBackground,
                Some(NSVisualEffectState::Active),
                None,
            )
            .expect("Vibrancy : uniquement supporte sur macOS");

            // Windows 11 : Mica natif. None => suit le theme clair/sombre du systeme.
            #[cfg(target_os = "windows")]
            apply_mica(&window, None)
                .expect("Mica : uniquement supporte sur Windows 11");

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![greet])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
