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

// macOS : par defaut WKWebView etire un snapshot ancre dans un coin pendant
// l'animation de zoom/resize, d'ou le contenu qui "saute" puis se recentre.
// On force chaque NSView a redessiner pendant le resize
// (NSViewLayerContentsRedrawDuringViewResize = 2), recursivement.
#[cfg(target_os = "macos")]
unsafe fn force_redraw_during_resize(view: *mut objc2::runtime::AnyObject) {
    use objc2::msg_send;

    if view.is_null() {
        return;
    }
    let _: () = msg_send![view, setLayerContentsRedrawPolicy: 2isize];

    let subviews: *mut objc2::runtime::AnyObject = msg_send![view, subviews];
    if subviews.is_null() {
        return;
    }
    let count: usize = msg_send![subviews, count];
    for i in 0..count {
        let sub: *mut objc2::runtime::AnyObject = msg_send![subviews, objectAtIndex: i];
        force_redraw_during_resize(sub);
    }
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

            // Corrige le "saut" du contenu pendant l'animation de zoom macOS.
            #[cfg(target_os = "macos")]
            unsafe {
                if let Ok(ns_window) = window.ns_window() {
                    let content_view: *mut objc2::runtime::AnyObject =
                        objc2::msg_send![ns_window as *mut objc2::runtime::AnyObject, contentView];
                    force_redraw_during_resize(content_view);
                }
            }

            // Windows 11 : on reste frameless (titlebar custom) => decorations OFF.
            // (decorations vaut true par defaut en config car requis pour la barre
            // native macOS ; on l'enleve ici uniquement cote Windows.)
            #[cfg(target_os = "windows")]
            {
                let _ = window.set_decorations(false);
                // Mica natif. None => suit le theme clair/sombre du systeme.
                apply_mica(&window, None)
                    .expect("Mica : uniquement supporte sur Windows 11");
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![greet])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
