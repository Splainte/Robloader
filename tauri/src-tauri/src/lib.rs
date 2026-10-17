use tauri::Manager;

mod engine;
mod i18n;
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

// macOS : choix de la langue au 1er lancement (feuille native sur la fenetre).
#[tauri::command]
fn mac_first_run(app: tauri::AppHandle) {
    #[cfg(target_os = "macos")]
    macos_ui::first_run(&app);
    #[cfg(not(target_os = "macos"))]
    let _ = app;
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

// Une panique dans un callback AppKit fait avorter l'app et le rapport de crash
// macOS ne contient pas son message : on l'ecrit dans
// ~/Library/Logs/Robloader/panic.log (macOS) pour pouvoir diagnostiquer.
fn log_panics() {
    #[cfg(target_os = "macos")]
    {
        let default_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            if let Ok(home) = std::env::var("HOME") {
                let dir = std::path::Path::new(&home).join("Library/Logs/Robloader");
                let _ = std::fs::create_dir_all(&dir);
                if let Ok(mut f) = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(dir.join("panic.log"))
                {
                    use std::io::Write;
                    let secs = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(0);
                    let _ = writeln!(
                        f,
                        "[{secs}] v{} {info}\n{}\n",
                        env!("CARGO_PKG_VERSION"),
                        std::backtrace::Backtrace::force_capture()
                    );
                }
            }
            default_hook(info);
        }));
    }
}

// macOS : menu de l'app, traduit (le menu par defaut de Tauri est en anglais).
// Refait a chaque changement de langue (cf. i18n::apply).
#[cfg(target_os = "macos")]
pub(crate) fn build_menu(app: &tauri::AppHandle) -> tauri::Result<()> {
    use i18n::tr;
    use tauri::menu::{AboutMetadata, Menu, MenuItem, PredefinedMenuItem, Submenu};
    let settings = MenuItem::with_id(app, "settings", tr("Réglages…", "Settings…"), true, Some("Cmd+,"))?;
    let app_menu = Submenu::with_items(
        app,
        "Robloader",
        true,
        &[
            &PredefinedMenuItem::about(
                app,
                Some(tr("À propos de Robloader", "About Robloader")),
                Some(AboutMetadata::default()),
            )?,
            &PredefinedMenuItem::separator(app)?,
            &settings,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::services(app, Some(tr("Services", "Services")))?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::hide(app, Some(tr("Masquer Robloader", "Hide Robloader")))?,
            &PredefinedMenuItem::hide_others(app, Some(tr("Masquer les autres", "Hide Others")))?,
            &PredefinedMenuItem::show_all(app, Some(tr("Tout afficher", "Show All")))?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::quit(app, Some(tr("Quitter Robloader", "Quit Robloader")))?,
        ],
    )?;
    let edit_menu = Submenu::with_items(
        app,
        tr("Édition", "Edit"),
        true,
        &[
            &PredefinedMenuItem::undo(app, Some(tr("Annuler", "Undo")))?,
            &PredefinedMenuItem::redo(app, Some(tr("Rétablir", "Redo")))?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::cut(app, Some(tr("Couper", "Cut")))?,
            &PredefinedMenuItem::copy(app, Some(tr("Copier", "Copy")))?,
            &PredefinedMenuItem::paste(app, Some(tr("Coller", "Paste")))?,
            &PredefinedMenuItem::select_all(app, Some(tr("Tout sélectionner", "Select All")))?,
        ],
    )?;
    let window_menu = Submenu::with_items(
        app,
        tr("Fenêtre", "Window"),
        true,
        &[
            &PredefinedMenuItem::minimize(app, Some(tr("Réduire", "Minimize")))?,
            &PredefinedMenuItem::maximize(app, Some(tr("Zoom", "Zoom")))?,
            &PredefinedMenuItem::fullscreen(app, Some(tr("Plein écran", "Enter Full Screen")))?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::close_window(app, Some(tr("Fermer la fenêtre", "Close Window")))?,
        ],
    )?;
    // « Test visuel » : file factice pour tester le defilement sous la barre
    // d'outils et ses interactions.
    let fill = MenuItem::with_id(app, "visual-test-fill", tr("Remplir la file (factice)", "Fill the queue (fake)"), true, None::<&str>)?;
    let clear = MenuItem::with_id(app, "visual-test-clear", tr("Vider la file factice", "Clear the fake queue"), true, None::<&str>)?;
    let test_menu = Submenu::with_items(app, tr("Test visuel", "Visual test"), true, &[&fill, &clear])?;
    app.set_menu(Menu::with_items(app, &[&app_menu, &edit_menu, &window_menu, &test_menu])?)?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    log_panics();
    // Avant la creation de la fenetre (cf. i18n::detect_veteran).
    let veteran = i18n::detect_veteran("com.robin.robloader");
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(engine::Engine::default())
        .setup(move |app| {
            // Langue avant toute interface (l'interface native macOS est
            // construite ci-dessous).
            i18n::init(veteran);
            let window = app.get_webview_window("main").unwrap();

            // La fenetre demarre cachee (visible: false) et le front l'affiche une
            // fois son fond dessine : tout apparait d'un coup. Filet de securite
            // si le front ne s'est pas manifeste au bout de 3 s.
            {
                let w = window.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_secs(3));
                    let _ = w.show();
                });
            }

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

            // macOS : menu de l'app (Reglages… ⌘, et « Test visuel »).
            #[cfg(target_os = "macos")]
            {
                use tauri::Emitter;
                build_menu(app.handle())?;
                app.on_menu_event(|app, event| match event.id().as_ref() {
                    "settings" => macos_ui::open_settings(app),
                    "visual-test-fill" => {
                        let _ = app.emit("mac://visual-test", true);
                    }
                    "visual-test-clear" => {
                        let _ = app.emit("mac://visual-test", false);
                    }
                    _ => {}
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
                // Layout a volet lateral (308 px) : fenetre plus large par defaut.
                {
                    use tauri::LogicalSize;
                    let _ = window.set_min_size(Some(LogicalSize::new(820.0, 560.0)));
                    let _ = window.set_size(LogicalSize::new(1060.0, 700.0));
                    let _ = window.center();
                }
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
            engine::open_source_page,
            i18n::get_language,
            i18n::set_language,
            mac_native_ui,
            mac_first_run,
            mac_set_env,
            mac_set_update,
            mac_toolbar_height,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
