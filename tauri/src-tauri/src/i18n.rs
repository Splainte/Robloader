// ============================================================
//  Robloader — langue de l'interface cote Rust
// ============================================================
//
// La langue vit ici : elle doit etre connue avant que le front charge, car
// l'interface native macOS (macos_ui.rs) est construite pendant le setup.
// Elle est enregistree dans config.json (cf. engine.rs) et le front la lit au
// demarrage (`get_language`), puis la change via `set_language`.
//
// Cote Rust : statuts de la file, messages d'erreur, interface native macOS
// et menus macOS.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::engine;

static ENGLISH: AtomicBool = AtomicBool::new(false);
// Vrai au tout premier lancement quand rien n'a fixe la langue (macOS : pas
// d'installeur) : l'app propose alors de la choisir.
static PICK_NEEDED: AtomicBool = AtomicBool::new(false);

pub fn en() -> bool {
    ENGLISH.load(Ordering::Relaxed)
}

pub fn lang() -> &'static str {
    if en() {
        "en"
    } else {
        "fr"
    }
}

// Texte fixe dans la langue courante : `tr("Annuler", "Cancel")`. Pour un texte
// avec variables, `if i18n::en() { format!(…) } else { format!(…) }`.
pub fn tr(fr: &'static str, en_: &'static str) -> &'static str {
    if en() {
        en_
    } else {
        fr
    }
}

// ------------------------------------------------------------
//  Langue au demarrage
// ------------------------------------------------------------

// Un utilisateur d'avant la traduction a deja lance l'app : la webview a cree
// ses donnees. A appeler AVANT la creation de la fenetre (debut de `run`),
// sinon la premiere ouverture de la nouvelle version les cree aussi.
pub fn detect_veteran(identifier: &str) -> bool {
    if engine::config_dir().join("config.json").exists() {
        return true;
    }
    webview_data_dirs(identifier).iter().any(|d| d.exists())
}

fn webview_data_dirs(identifier: &str) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if cfg!(windows) {
        // WebView2 : %LOCALAPPDATA%\<identifiant>\EBWebView
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            dirs.push(PathBuf::from(local).join(identifier));
        }
    } else if cfg!(target_os = "macos") {
        // WKWebView : ~/Library/WebKit/<identifiant> et son cache.
        if let Ok(home) = std::env::var("HOME") {
            let lib = PathBuf::from(home).join("Library");
            dirs.push(lib.join("WebKit").join(identifier));
            dirs.push(lib.join("Caches").join(identifier));
        }
    }
    dirs
}

// Ordre de priorite (repris de Copiteur) :
//   1. la langue choisie dans l'installeur Windows, si elle a CHANGE depuis la
//      derniere fois (reinstallation dans une autre langue) ;
//   2. celle enregistree ;
//   3. un utilisateur d'avant la traduction : francais, sans rien lui demander ;
//   4. sinon (1er lancement sans installeur) : langue du systeme, et l'app
//      propose de la changer.
pub fn init(veteran: bool) {
    let mut cfg = engine::load_config();
    let installer = installer_language();
    let saved = cfg.language.clone().filter(|l| l == "fr" || l == "en");

    let (lang, pick) = match (&installer, &saved) {
        (Some(i), _) if cfg.installer_language.as_deref() != Some(i) && !(saved.is_none() && veteran) => {
            (i.clone(), false)
        }
        (_, Some(s)) => (s.clone(), false),
        _ if veteran => ("fr".to_string(), false),
        _ => (system_language().to_string(), true),
    };
    ENGLISH.store(lang == "en", Ordering::Relaxed);
    PICK_NEEDED.store(pick, Ordering::Relaxed);

    // Le choix du 1er lancement n'est enregistre qu'a la reponse : si l'app est
    // quittee sans repondre, la question sera reposee.
    let mut dirty = false;
    if installer.is_some() && cfg.installer_language != installer {
        cfg.installer_language = installer;
        dirty = true;
    }
    if !pick && cfg.language.as_deref() != Some(lang.as_str()) {
        cfg.language = Some(lang);
        dirty = true;
    }
    if dirty {
        engine::save_config(&cfg);
    }
}

// Langue du systeme : francais si le systeme l'est, anglais sinon.
fn system_language() -> &'static str {
    let code = system_language_code().to_lowercase();
    if code.starts_with("fr") {
        "fr"
    } else {
        "en"
    }
}

#[cfg(target_os = "macos")]
fn system_language_code() -> String {
    use objc2::msg_send;
    use objc2::runtime::{AnyClass, AnyObject};
    let Some(cls) = AnyClass::get(c"NSLocale") else { return String::new() };
    unsafe {
        let langs: *mut AnyObject = msg_send![cls, preferredLanguages];
        if langs.is_null() {
            return String::new();
        }
        let count: usize = msg_send![langs, count];
        if count == 0 {
            return String::new();
        }
        let first: *mut AnyObject = msg_send![langs, objectAtIndex: 0usize];
        let s: &objc2_foundation::NSString = &*(first as *const objc2_foundation::NSString);
        s.to_string()
    }
}

#[cfg(windows)]
fn system_language_code() -> String {
    #[link(name = "kernel32")]
    extern "system" {
        fn GetUserDefaultUILanguage() -> u16;
    }
    // Langue principale = 10 bits de poids faible (0x0C francais).
    if unsafe { GetUserDefaultUILanguage() } & 0x3ff == 0x0c {
        "fr".into()
    } else {
        "en".into()
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
fn system_language_code() -> String {
    std::env::var("LANG").unwrap_or_default()
}

// Langue choisie dans l'installeur Windows (« fr » / « en »), s'il y en a une.
// L'installeur NSIS de Tauri la range dans HKCU\Software\<editeur>\<produit>,
// valeur « Installer Language » (identifiant de langue Windows, en texte).
// L'editeur n'est pas renseigne dans tauri.conf.json : Tauri prend alors le
// 2e segment de l'identifiant (com.robin.robloader -> robin).
fn installer_language() -> Option<String> {
    #[cfg(windows)]
    {
        let id: u32 = installer_langid()?.trim().parse().ok()?;
        match id & 0x3ff {
            0x0c => Some("fr".into()),
            0x09 => Some("en".into()),
            _ => None,
        }
    }
    #[cfg(not(windows))]
    None
}

#[cfg(windows)]
fn installer_langid() -> Option<String> {
    engine::hkcu_string("Software\\robin\\Robloader", "Installer Language")
}

// ------------------------------------------------------------
//  Commandes
// ------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LanguageInfo {
    lang: &'static str,
    pick_needed: bool,
}

#[tauri::command]
pub fn get_language() -> LanguageInfo {
    LanguageInfo {
        lang: lang(),
        pick_needed: PICK_NEEDED.load(Ordering::Relaxed),
    }
}

// Change et enregistre la langue. Appelee par le front (reglages Windows) ou
// par l'interface native macOS ; dans les deux cas le front est prevenu par
// l'evenement « app://language » et se re-rend dans la nouvelle langue.
#[tauri::command]
pub fn set_language(app: AppHandle, lang: String) {
    apply(&app, &lang);
}

pub fn apply(app: &AppHandle, lang: &str) {
    let english = lang == "en";
    ENGLISH.store(english, Ordering::Relaxed);
    PICK_NEEDED.store(false, Ordering::Relaxed);
    let mut cfg = engine::load_config();
    cfg.language = Some(self::lang().to_string());
    engine::save_config(&cfg);

    #[cfg(target_os = "macos")]
    {
        let handle = app.clone();
        let _ = app.run_on_main_thread(move || {
            let _ = crate::build_menu(&handle);
            crate::macos_ui::relabel();
        });
        // Langue des elements fournis par macOS (boite « Choisir un dossier »…) :
        // lue au LANCEMENT, d'ou l'ecriture dans les preferences de l'app pour
        // la prochaine fois.
        let id = app.config().identifier.clone();
        std::thread::spawn(move || {
            let _ = std::process::Command::new("defaults")
                .args(["write", &id, "AppleLanguages", "-array", if english { "en" } else { "fr" }])
                .status();
        });
    }
    let _ = app.emit("app://language", self::lang());
}
