use tauri::Manager;

mod engine;

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

// macOS 26 (Tahoe) : VRAI "Liquid Glass" via NSGlassEffectView (AppKit), avec
// la refraction optique sur les bords. La classe n'existe qu'a partir de macOS
// 26 : si elle est absente, on renvoie false et l'appelant retombe sur la
// Vibrancy classique (NSVisualEffectView). Aucune liaison a la compilation : on
// resout la classe dynamiquement, donc ca compile sans le SDK 26.
//
// NOTE (a valider sur Mac reel) : NSGlassEffectView est concu pour ENVELOPPER
// un contenu (contentView). On l'utilise ici comme vue de fond plein cadre,
// sous la WebView transparente. Si le verre ne s'affiche pas dans cette
// configuration, plan B = reparenter la WebView dans le contentView du verre.
#[cfg(target_os = "macos")]
unsafe fn apply_liquid_glass(window: &tauri::WebviewWindow) -> bool {
    use objc2::runtime::{AnyClass, AnyObject};
    use objc2::msg_send;
    use objc2_core_foundation::CGRect;

    // Presence de la classe = macOS 26+. Absente => OS trop ancien.
    let Some(glass_class) = AnyClass::get(c"NSGlassEffectView") else {
        return false;
    };

    let Ok(ns_window) = window.ns_window() else {
        return false;
    };
    let ns_window = ns_window as *mut AnyObject;
    let content_view: *mut AnyObject = msg_send![ns_window, contentView];
    if content_view.is_null() {
        return false;
    }
    let bounds: CGRect = msg_send![content_view, bounds];

    // Instancie le verre, etire sur toute la fenetre.
    let glass: *mut AnyObject = msg_send![glass_class, alloc];
    let glass: *mut AnyObject = msg_send![glass, initWithFrame: bounds];
    if glass.is_null() {
        return false;
    }
    // Suit le redimensionnement : NSViewWidthSizable(2) | NSViewHeightSizable(16).
    let _: () = msg_send![glass, setAutoresizingMask: 18usize];
    // Coins arrondis assortis a la fenetre (a ajuster au rendu reel).
    let _: () = msg_send![glass, setCornerRadius: 10.0f64];

    // Insere le verre tout au fond, sous la WebView (NSWindowBelow = -1).
    let nil = std::ptr::null::<AnyObject>();
    let _: () = msg_send![content_view, addSubview: glass, positioned: -1isize, relativeTo: nil];
    true
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

            // macOS 26 : vrai Liquid Glass natif (NSGlassEffectView). Sur les
            // versions anterieures, fallback automatique sur la Vibrancy
            // classique (NSVisualEffectView). None pour theme/state laisse le
            // systeme suivre l'apparence claire/sombre.
            #[cfg(target_os = "macos")]
            {
                let glass_applied = unsafe { apply_liquid_glass(&window) };
                if !glass_applied {
                    apply_vibrancy(
                        &window,
                        NSVisualEffectMaterial::UnderWindowBackground,
                        Some(NSVisualEffectState::Active),
                        None,
                    )
                    .expect("Vibrancy : uniquement supporte sur macOS");
                }
            }

            // Corrige le "saut" du contenu pendant l'animation de zoom macOS.
            #[cfg(target_os = "macos")]
            unsafe {
                if let Ok(ns_window) = window.ns_window() {
                    let content_view: *mut objc2::runtime::AnyObject =
                        objc2::msg_send![ns_window as *mut objc2::runtime::AnyObject, contentView];
                    stabilize_content_on_resize(content_view);
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
