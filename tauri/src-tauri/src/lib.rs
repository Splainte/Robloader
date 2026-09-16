use tauri::Manager;

mod engine;
#[cfg(target_os = "macos")]
mod macos_ui;

#[cfg(target_os = "macos")]
use window_vibrancy::{apply_vibrancy, NSVisualEffectMaterial, NSVisualEffectState};
#[cfg(target_os = "windows")]
use window_vibrancy::apply_mica;

// macOS : le contenu centre "saute" (ancre dans un coin) pendant resize/zoom.
// Deux cas a couvrir sur chaque NSView, recursivement :
//  - live resize (tirer les coins) : redessiner pendant le resize
//    => NSViewLayerContentsRedrawDuringViewResize = 2
//  - animation de zoom (double-clic barre) : ce n'est PAS un live resize, la
//    couche repositionne son ancien bitmap selon layerContentsPlacement, ancre
//    dans un coin par defaut => on force le placement au centre
//    => NSViewLayerContentsPlacementCenter = 3
#[cfg(target_os = "macos")]
unsafe fn stabilize_content_on_resize(view: *mut objc2::runtime::AnyObject) {
    use objc2::msg_send;

    if view.is_null() {
        return;
    }
    let _: () = msg_send![view, setLayerContentsRedrawPolicy: 2isize];
    let _: () = msg_send![view, setLayerContentsPlacement: 3isize];

    let subviews: *mut objc2::runtime::AnyObject = msg_send![view, subviews];
    if subviews.is_null() {
        return;
    }
    let count: usize = msg_send![subviews, count];
    for i in 0..count {
        let sub: *mut objc2::runtime::AnyObject = msg_send![subviews, objectAtIndex: i];
        stabilize_content_on_resize(sub);
    }
}

// Fond opaque de secours quand le materiau natif (Mica/Vibrancy) est
// indisponible : la page a un fond CSS 100% transparent, sans materiau la
// fenetre serait invisible/illisible.
#[cfg(any(target_os = "macos", target_os = "windows"))]
fn fallback_solid_background<R: tauri::Runtime>(window: &tauri::WebviewWindow<R>) {
    use tauri::webview::Color;
    let dark = matches!(window.theme(), Ok(tauri::Theme::Dark));
    let color = if dark {
        Color(32, 32, 32, 255)
    } else {
        Color(243, 243, 243, 255)
    };
    let _ = window.set_background_color(Some(color));
}

// ---------- Pont avec l'interface native macOS (sans effet ailleurs) ----------

#[tauri::command]
fn mac_native_ui() -> bool {
    #[cfg(target_os = "macos")]
    return macos_ui::native_ui_installed();
    #[cfg(not(target_os = "macos"))]
    false
}

#[tauri::command]
fn mac_set_env(
    app: tauri::AppHandle,
    download_dir: String,
    cookies_ok: bool,
    cookies_source: String,
    js_runtime: bool,
) {
    #[cfg(target_os = "macos")]
    macos_ui::set_env(app, download_dir, cookies_ok, cookies_source, js_runtime);
    #[cfg(not(target_os = "macos"))]
    let _ = (app, download_dir, cookies_ok, cookies_source, js_runtime);
}

#[tauri::command]
fn mac_set_update(app: tauri::AppHandle, version: Option<String>, installing: bool) {
    #[cfg(target_os = "macos")]
    macos_ui::set_update(app, version, installing);
    #[cfg(not(target_os = "macos"))]
    let _ = (app, version, installing);
}

#[tauri::command]
async fn mac_toolbar_height(app: tauri::AppHandle) -> f64 {
    #[cfg(target_os = "macos")]
    return macos_ui::toolbar_height(app);
    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
        0.0
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(engine::Engine::default())
        .setup(|app| {
            let window = app.get_webview_window("main").unwrap();

            // Sur Linux, aucun materiau natif : on evite le warning "unused".
            #[cfg(not(any(target_os = "macos", target_os = "windows")))]
            let _ = &window;

            // Corrige le "saut" du contenu pendant l'animation de zoom macOS.
            // Applique AVANT l'interface native : seulement la webview et son
            // conteneur, pas les controles AppKit (NSSwitch, verre...).
            #[cfg(target_os = "macos")]
            unsafe {
                if let Ok(ns_window) = window.ns_window() {
                    let content_view: *mut objc2::runtime::AnyObject =
                        objc2::msg_send![ns_window as *mut objc2::runtime::AnyObject, contentView];
                    stabilize_content_on_resize(content_view);
                }
            }

            // macOS : barre d'outils + volet lateral natifs (Liquid Glass). La
            // webview ne garde que la file, sur fond plein. Si l'installation
            // echoue, on retombe sur l'ancien fond Vibrancy plein cadre.
            // None pour theme/state laisse le systeme suivre l'apparence claire/sombre.
            #[cfg(target_os = "macos")]
            if !macos_ui::install(app.handle(), &window)
                && apply_vibrancy(
                &window,
                NSVisualEffectMaterial::UnderWindowBackground,
                Some(NSVisualEffectState::Active),
                None,
            )
            .is_err()
            {
                fallback_solid_background(&window);
            }

            // macOS : menu « Test visuel » (file factice pour tester le
            // defilement sous la barre d'outils et ses interactions).
            #[cfg(target_os = "macos")]
            {
                use tauri::menu::{Menu, MenuItem, Submenu};
                use tauri::Emitter;
                let handle = app.handle();
                let menu = Menu::default(handle)?;
                let fill = MenuItem::with_id(handle, "visual-test-fill", "Remplir la file (factice)", true, None::<&str>)?;
                let clear = MenuItem::with_id(handle, "visual-test-clear", "Vider la file factice", true, None::<&str>)?;
                menu.append(&Submenu::with_items(handle, "Test visuel", true, &[&fill, &clear])?)?;
                app.set_menu(menu)?;
                app.on_menu_event(|app, event| {
                    let fill = match event.id().as_ref() {
                        "visual-test-fill" => true,
                        "visual-test-clear" => false,
                        _ => return,
                    };
                    let _ = app.emit("mac://visual-test", fill);
                });
            }

            // macOS : le layout a volet lateral (290 px) exige une fenetre plus
            // large que le minimum commun (560 px, garde pour Windows).
            #[cfg(target_os = "macos")]
            {
                use tauri::LogicalSize;
                let _ = window.set_min_size(Some(LogicalSize::new(780.0, 520.0)));
                let _ = window.set_size(LogicalSize::new(1000.0, 680.0));
                let _ = window.center();
            }

            // Windows 11 : on reste frameless (titlebar custom) => decorations OFF.
            // (decorations vaut true par defaut en config car requis pour la barre
            // native macOS ; on l'enleve ici uniquement cote Windows.)
            #[cfg(target_os = "windows")]
            {
                let _ = window.set_decorations(false);
                // Mica natif. None => suit le theme clair/sombre du systeme.
                // Windows 10 : Mica n'existe pas — comme le fond CSS est 100%
                // transparent, on peint un fond opaque au lieu de paniquer
                // (avant ce correctif l'app crashait au lancement sur Win10).
                if apply_mica(&window, None).is_err() {
                    fallback_solid_background(&window);
                }
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            engine::get_env,
            engine::get_accent_color,
            engine::check_update,
            engine::install_update,
            engine::set_download_dir,
            engine::choose_destination,
            engine::start_download,
            engine::cancel_download,
            engine::reveal_in_folder,
            engine::open_cookie_help,
            mac_native_ui,
            mac_set_env,
            mac_set_update,
            mac_toolbar_height,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
