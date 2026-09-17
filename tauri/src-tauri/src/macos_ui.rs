// ============================================================
// Interface native macOS (redesign D3, cf. docs/redesign-macos).
//
// - NSToolbar : titre au-dessus du volet, capsule de lien facon Safari,
//   source, Coller, Mise a jour, Telecharger (style prominent macOS 26+).
// - NSSplitViewController : volet lateral systeme (Liquid Glass, suit le
//   reglage d'opacite de macOS 27) avec des controles AppKit natifs
//   (NSButton, NSSwitch, NSPopUpButton...). La webview Tauri devient le
//   panneau de contenu et n'affiche plus que la file.
//
// Les reglages vivent ici (SETTINGS). Un clic sur Telecharger (ou Entree)
// emet "mac://download" avec le lien + les reglages ; le front cree la
// tache et appelle start_download comme avant.
// Tout ce qui touche AppKit tourne sur le thread principal.
// ============================================================

use std::cell::{Cell, OnceCell, RefCell};
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject, ProtocolObject, Sel};
use objc2::{define_class, msg_send, sel, DefinedClass, MainThreadOnly};
use objc2_app_kit::{
    NSApplication, NSButton, NSColor, NSControlStateValueOff,
    NSControlStateValueOn, NSControlTextEditingDelegate, NSEvent, NSEventType, NSFocusRingType,
    NSFont, NSFontWeightBold, NSFontWeightSemibold, NSImage, NSImageView, NSLayoutAttribute,
    NSLayoutConstraint, NSLayoutConstraintOrientation, NSLineBreakMode, NSPasteboard,
    NSPasteboardTypeString, NSPopUpButton, NSScrollView, NSSplitViewController, NSSplitViewItem,
    NSSplitViewItemBehavior, NSStackView, NSStackViewDistribution, NSSwitch, NSTextField,
    NSTextFieldDelegate, NSToolbar, NSToolbarDelegate, NSToolbarDisplayMode,
    NSToolbarFlexibleSpaceItemIdentifier, NSToolbarItem, NSToolbarSpaceItemIdentifier, NSToolbarItemStyle,
    NSToolbarSidebarTrackingSeparatorItemIdentifier, NSUserInterfaceLayoutOrientation, NSView,
    NSViewController, NSWindow, NSWindowTitleVisibility, NSWindowToolbarStyle,
};
use objc2_foundation::{
    ns_string, MainThreadMarker, NSArray, NSEdgeInsets, NSNotification, NSObject,
    NSNumber, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString,
};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

// ---------- Donnees (miroir de App.tsx) ----------

const QUALITY_LABELS: [&str; 5] = [
    "Qualité max (jusqu'à 4K)",
    "1440p (QHD)",
    "1080p (Full HD)",
    "720p (HD)",
    "480p",
];
const QUALITY_SHORT: [&str; 5] = ["Max", "1440p", "1080p", "720p", "480p"];
const OUTPUTS_TRANSCODE: [&str; 4] = ["HEVC", "ProRes", "Audio WAV", "Sous-titres seuls (.srt)"];
const OUTPUTS_NATIVE: [&str; 3] = ["Vidéo (natif)", "Audio WAV", "Sous-titres seuls (.srt)"];

struct Profile {
    id: &'static str,
    label: &'static str,
    domains: &'static [&'static str],
    placeholder: &'static str,
    ladder: bool,
    subtitles: bool,
    thumbnail: bool,
}

const DEFAULT_PROFILE: Profile = Profile {
    id: "default",
    label: "Vidéo",
    domains: &[],
    placeholder: "Colle un lien (YouTube, TikTok, Instagram, X, Weibo)…",
    ladder: true,
    subtitles: true,
    thumbnail: true,
};
const SITE_PROFILES: [Profile; 5] = [
    Profile { id: "youtube", label: "YouTube", domains: &["youtube.com", "youtu.be"], placeholder: "Colle un lien YouTube ici…", ladder: true, subtitles: true, thumbnail: true },
    Profile { id: "tiktok", label: "TikTok", domains: &["tiktok.com"], placeholder: "Colle un lien TikTok ici…", ladder: false, subtitles: false, thumbnail: true },
    Profile { id: "instagram", label: "Instagram", domains: &["instagram.com", "instagr.am"], placeholder: "Colle un lien Instagram ici…", ladder: false, subtitles: false, thumbnail: false },
    Profile { id: "x", label: "X", domains: &["twitter.com", "x.com"], placeholder: "Colle un lien X (Twitter) ici…", ladder: false, subtitles: false, thumbnail: false },
    Profile { id: "weibo", label: "Weibo", domains: &["weibo.com", "weibo.cn"], placeholder: "Colle un lien Weibo ici…", ladder: false, subtitles: false, thumbnail: false },
];

fn detect_profile(url: &str) -> &'static Profile {
    let u = url.trim();
    let rest = u.split_once("://").map(|(_, r)| r).unwrap_or(u);
    let host = rest
        .split(['/', '?', '#'])
        .next()
        .unwrap_or("")
        .rsplit('@')
        .next()
        .unwrap_or("")
        .split(':')
        .next()
        .unwrap_or("")
        .to_lowercase();
    if host.is_empty() {
        return &DEFAULT_PROFILE;
    }
    SITE_PROFILES
        .iter()
        .find(|p| {
            p.domains
                .iter()
                .any(|d| host == *d || host.ends_with(&format!(".{d}")))
        })
        .unwrap_or(&DEFAULT_PROFILE)
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    quality_label: String,
    clip: bool,
    start: String,
    end: String,
    transcode: bool,
    output: String,
    subs: bool,
    thumb: bool,
}

static SETTINGS: Mutex<Option<Settings>> = Mutex::new(None);
static INSTALLED: AtomicBool = AtomicBool::new(false);

fn default_settings() -> Settings {
    Settings {
        quality_label: QUALITY_LABELS[0].into(),
        clip: false,
        start: String::new(),
        end: String::new(),
        transcode: true,
        output: OUTPUTS_TRANSCODE[0].into(),
        subs: false,
        thumb: false,
    }
}

fn with_settings<T>(f: impl FnOnce(&mut Settings) -> T) -> T {
    let mut guard = SETTINGS.lock().unwrap();
    f(guard.get_or_insert_with(default_settings))
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DownloadRequest {
    url: String,
    settings: Settings,
}

// ---------- Zone du volet affichee/masquee ----------
//
// Un conteneur a hauteur contrainte (0 = ferme) masque son contenu.
// Les marges sont DANS le conteneur, pour qu'une zone fermee ne laisse aucun
// espace dans la pile.

struct Reveal {
    container: Retained<NSView>,
    content: Retained<NSView>,
    height: Retained<NSLayoutConstraint>,
    pad_top: f64,
    pad_bottom: f64,
}

impl Reveal {
    fn new(content: &NSView, pad_top: f64, pad_bottom: f64, open: bool, mtm: MainThreadMarker) -> Self {
        let container = NSView::new(mtm);
        container.setWantsLayer(true);
        unsafe {
            let layer: *mut AnyObject = msg_send![&*container, layer];
            if !layer.is_null() {
                let _: () = msg_send![layer, setMasksToBounds: true];
            }
        }
        container.setTranslatesAutoresizingMaskIntoConstraints(false);
        content.setTranslatesAutoresizingMaskIntoConstraints(false);
        container.addSubview(content);
        let pins = [
            content.topAnchor().constraintEqualToAnchor_constant(&container.topAnchor(), pad_top),
            content.leadingAnchor().constraintEqualToAnchor(&container.leadingAnchor()),
            content.trailingAnchor().constraintEqualToAnchor(&container.trailingAnchor()),
        ];
        NSLayoutConstraint::activateConstraints(&NSArray::from_retained_slice(&pins));
        let height = container.heightAnchor().constraintEqualToConstant(0.0);
        height.setActive(true);
        let reveal = Reveal {
            container,
            content: retain_ref(content),
            height,
            pad_top,
            pad_bottom,
        };
        reveal.height.setConstant(if open { reveal.open_height() } else { 0.0 });
        reveal.container.setAlphaValue(if open { 1.0 } else { 0.0 });
        reveal
    }

    fn open_height(&self) -> f64 {
        self.content.fittingSize().height + self.pad_top + self.pad_bottom
    }

    fn is_open(&self) -> bool {
        self.height.constant() > 0.5
    }

    // Sans animation (choix de Robin) : la zone apparait/disparait d'un coup.
    fn set(&self, open: bool) {
        if self.is_open() == open {
            return;
        }
        self.height.setConstant(if open { self.open_height() } else { 0.0 });
        self.container.setAlphaValue(if open { 1.0 } else { 0.0 });
    }
}

// ---------- Vues gardees pour les mises a jour (thread principal) ----------

struct Ui {
    window: Retained<NSWindow>,
    url_capsule: Retained<UrlCapsule>,
    url_field: Retained<NSTextField>,
    chip_item: Option<Retained<NSToolbarItem>>,
    update_item: Option<Retained<NSToolbarItem>>,
    quality_reveal: Reveal,
    quality_buttons: Vec<Retained<NSButton>>,
    clip_switch: Retained<NSSwitch>,
    times_reveal: Reveal,
    start_field: Retained<NSTextField>,
    end_field: Retained<NSTextField>,
    transcode_switch: Retained<NSSwitch>,
    output_reveal: Reveal,
    output_popup: Retained<NSPopUpButton>,
    subs_reveal: Reveal,
    subs_switch: Retained<NSSwitch>,
    thumb_reveal: Reveal,
    thumb_switch: Retained<NSSwitch>,
    dest_label: Retained<NSTextField>,
    foot_label: Retained<NSTextField>,
    repair_button: Retained<NSButton>,
    profile_id: &'static str,
}

thread_local! {
    static UI: RefCell<Option<Ui>> = const { RefCell::new(None) };
    static CONTROLLER: RefCell<Option<Retained<Controller>>> = const { RefCell::new(None) };
}

// Pas d'appel AppKit susceptible de rappeler un delegue (makeFirstResponder…)
// dans `f` : le rappel retomberait ici pendant l'emprunt. Une panique dans un
// callback Obj-C ne peut pas remonter et avorte l'app : on ignore alors l'appel
// imbrique au lieu de paniquer.
fn with_ui<T>(f: impl FnOnce(&mut Ui) -> T) -> Option<T> {
    UI.with(|cell| cell.try_borrow_mut().ok()?.as_mut().map(f))
}

// Donne le focus au champ de lien hors de tout emprunt de UI : si le champ est
// deja en edition, AppKit envoie controlTextDidEndEditing de facon synchrone.
fn focus_url_field() {
    if let Some((window, field)) = with_ui(|ui| (ui.window.clone(), ui.url_field.clone())) {
        window.makeFirstResponder(Some(&field));
    }
}

// ---------- Capsule du champ de lien (facon barre d'adresse Safari) ----------

pub struct CapsuleIvars {
    focused: Cell<bool>,
    field: OnceCell<Retained<NSTextField>>,
}

define_class!(
    #[unsafe(super(NSView))]
    #[thread_kind = MainThreadOnly]
    #[name = "RobloaderUrlCapsule"]
    #[ivars = CapsuleIvars]
    pub struct UrlCapsule;

    impl UrlCapsule {
        #[unsafe(method(wantsUpdateLayer))]
        fn wants_update_layer(&self) -> bool {
            true
        }

        #[unsafe(method(updateLayer))]
        fn update_layer(&self) {
            self.paint();
        }

        #[unsafe(method(viewDidChangeEffectiveAppearance))]
        fn appearance_changed(&self) {
            self.setNeedsDisplay(true);
        }

        #[unsafe(method(mouseDownCanMoveWindow))]
        fn mouse_down_can_move_window(&self) -> bool {
            false
        }

        // Clic dans la capsule hors du texte : focus du champ.
        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, _event: &NSEvent) {
            if let (Some(window), Some(field)) = (self.window(), self.ivars().field.get()) {
                window.makeFirstResponder(Some(field));
            }
        }
    }
);

impl UrlCapsule {
    fn new(field: &NSTextField, mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(CapsuleIvars {
            focused: Cell::new(false),
            field: OnceCell::new(),
        });
        let this: Retained<Self> = unsafe {
            msg_send![super(this), initWithFrame: NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(320.0, 36.0))]
        };
        let _ = this.ivars().field.set(retain_ref(field));
        this.setWantsLayer(true);
        this
    }

    fn set_focused(&self, focused: bool) {
        if self.ivars().focused.replace(focused) != focused {
            self.setNeedsDisplay(true);
        }
    }

    fn paint(&self) {
        // Pas de fond : sur macOS 26/27 la barre d'outils enveloppe deja l'item
        // dans son verre (un fond en plus le rendait opaque). Seul le contour
        // d'accent au focus est dessine ici.
        let focused = self.ivars().focused.get();
        let fill = NSColor::clearColor();
        let border = if focused {
            NSColor::controlAccentColor()
        } else {
            NSColor::clearColor()
        };
        unsafe {
            let layer: *mut AnyObject = msg_send![self, layer];
            if layer.is_null() {
                return;
            }
            let fill_cg: *mut c_void = msg_send![&*fill, CGColor];
            let border_cg: *mut c_void = msg_send![&*border, CGColor];
            let _: () = msg_send![layer, setCornerRadius: 18.0f64];
            let _: () = msg_send![layer, setBackgroundColor: fill_cg];
            let _: () = msg_send![layer, setBorderColor: border_cg];
            let _: () = msg_send![layer, setBorderWidth: if focused { 2.5f64 } else { 0.0f64 }];
        }
    }
}

// ---------- Controleur Obj-C (delegue + cible des actions) ----------

pub struct Ivars {
    app: AppHandle,
    version: String,
}

const ID_TITLE: &str = "rl.title";
const ID_URL: &str = "rl.url";
const ID_CHIP: &str = "rl.chip";
const ID_PASTE: &str = "rl.paste";
const ID_UPDATE: &str = "rl.update";
const ID_DOWNLOAD: &str = "rl.download";

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "RobloaderNativeController"]
    #[ivars = Ivars]
    pub struct Controller;

    unsafe impl NSObjectProtocol for Controller {}

    unsafe impl NSToolbarDelegate for Controller {
        #[unsafe(method_id(toolbar:itemForItemIdentifier:willBeInsertedIntoToolbar:))]
        fn toolbar_item(
            &self,
            _toolbar: &NSToolbar,
            identifier: &NSString,
            _flag: bool,
        ) -> Option<Retained<NSToolbarItem>> {
            self.make_toolbar_item(identifier)
        }

        #[unsafe(method_id(toolbarDefaultItemIdentifiers:))]
        fn default_identifiers(&self, _toolbar: &NSToolbar) -> Retained<NSArray<NSString>> {
            toolbar_identifiers()
        }

        #[unsafe(method_id(toolbarAllowedItemIdentifiers:))]
        fn allowed_identifiers(&self, _toolbar: &NSToolbar) -> Retained<NSArray<NSString>> {
            toolbar_identifiers()
        }
    }

    unsafe impl NSControlTextEditingDelegate for Controller {
        #[unsafe(method(controlTextDidChange:))]
        fn control_text_did_change(&self, notification: &NSNotification) {
            let sender = notification.object();
            self.text_changed(sender.as_deref());
        }

        #[unsafe(method(controlTextDidBeginEditing:))]
        fn control_text_did_begin_editing(&self, notification: &NSNotification) {
            self.url_focus_changed(notification, true);
        }

        #[unsafe(method(controlTextDidEndEditing:))]
        fn control_text_did_end_editing(&self, notification: &NSNotification) {
            self.url_focus_changed(notification, false);
        }
    }

    unsafe impl NSTextFieldDelegate for Controller {}

    impl Controller {
        #[unsafe(method(onDownload:))]
        fn on_download(&self, _sender: Option<&AnyObject>) {
            self.request_download();
        }

        // Action des champs texte : seulement sur Entree (pas en perdant le focus).
        #[unsafe(method(onSubmit:))]
        fn on_submit(&self, _sender: Option<&AnyObject>) {
            if return_key_pressed(self.mtm()) {
                self.request_download();
            }
        }

        #[unsafe(method(onPaste:))]
        fn on_paste(&self, _sender: Option<&AnyObject>) {
            let text = unsafe {
                NSPasteboard::generalPasteboard().stringForType(NSPasteboardTypeString)
            };
            let Some(text) = text else { return };
            let text = text.to_string().trim().to_string();
            if text.is_empty() {
                return;
            }
            with_ui(|ui| ui.url_field.setStringValue(&NSString::from_str(&text)));
            focus_url_field();
            apply_profile(detect_profile(&text));
        }

        #[unsafe(method(onQuality:))]
        fn on_quality(&self, sender: Option<&AnyObject>) {
            let Some(button) = sender.and_then(|s| s.downcast_ref::<NSButton>()) else {
                return;
            };
            let idx = button.tag().clamp(0, 4) as usize;
            with_settings(|s| s.quality_label = QUALITY_LABELS[idx].into());
            with_ui(|ui| select_quality(&ui.quality_buttons, idx));
        }

        #[unsafe(method(onSwitch:))]
        fn on_switch(&self, sender: Option<&AnyObject>) {
            let Some(sw) = sender.and_then(|s| s.downcast_ref::<NSSwitch>()) else {
                return;
            };
            let on = sw.state() == NSControlStateValueOn;
            let ptr = sw as *const NSSwitch;
            let which = with_ui(|ui| {
                if ptr == Retained::as_ptr(&ui.clip_switch) {
                    1
                } else if ptr == Retained::as_ptr(&ui.transcode_switch) {
                    2
                } else if ptr == Retained::as_ptr(&ui.subs_switch) {
                    3
                } else if ptr == Retained::as_ptr(&ui.thumb_switch) {
                    4
                } else {
                    0
                }
            })
            .unwrap_or(0);
            match which {
                1 => {
                    with_settings(|s| s.clip = on);
                    with_ui(|ui| ui.times_reveal.set(on));
                }
                2 => {
                    let output = with_settings(|s| {
                        s.transcode = on;
                        let list: &[&str] = if on { &OUTPUTS_TRANSCODE } else { &OUTPUTS_NATIVE };
                        if !list.contains(&s.output.as_str()) {
                            s.output = list[0].into();
                        }
                        s.output.clone()
                    });
                    with_ui(|ui| {
                        fill_outputs(&ui.output_popup, on, &output);
                        ui.output_reveal.set(on);
                    });
                }
                3 => with_settings(|s| s.subs = on),
                4 => with_settings(|s| s.thumb = on),
                _ => {}
            }
        }

        #[unsafe(method(onOutput:))]
        fn on_output(&self, sender: Option<&AnyObject>) {
            let Some(popup) = sender.and_then(|s| s.downcast_ref::<NSPopUpButton>()) else {
                return;
            };
            if let Some(title) = popup.titleOfSelectedItem() {
                with_settings(|s| s.output = title.to_string());
            }
        }

        #[unsafe(method(onChooseDestination:))]
        fn on_choose_destination(&self, _sender: Option<&AnyObject>) {
            let _ = self.ivars().app.emit("mac://choose-destination", ());
        }

        #[unsafe(method(onRevealDestination:))]
        fn on_reveal_destination(&self, _sender: Option<&AnyObject>) {
            let _ = self.ivars().app.emit("mac://reveal-destination", ());
        }

        #[unsafe(method(onRepair:))]
        fn on_repair(&self, _sender: Option<&AnyObject>) {
            let _ = self.ivars().app.emit("mac://repair", ());
        }

        #[unsafe(method(onUpdate:))]
        fn on_update(&self, _sender: Option<&AnyObject>) {
            let _ = self.ivars().app.emit("mac://install-update", ());
        }
    }
);

impl Controller {
    fn new(app: AppHandle, version: String, mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(Ivars { app, version });
        unsafe { msg_send![super(this), init] }
    }

    fn as_target(&self) -> &AnyObject {
        self.as_ref()
    }

    fn make_toolbar_item(&self, identifier: &NSString) -> Option<Retained<NSToolbarItem>> {
        let mtm = self.mtm();
        let id = identifier.to_string();
        let item = NSToolbarItem::initWithItemIdentifier(NSToolbarItem::alloc(mtm), identifier);
        match id.as_str() {
            ID_TITLE => {
                // « Robloader 2.1.7 » a droite des feux, au-dessus du volet.
                let name = label("Robloader", mtm);
                name.setFont(Some(&NSFont::systemFontOfSize_weight(15.0, unsafe {
                    NSFontWeightBold
                })));
                let version = label(&self.ivars().version, mtm);
                version.setFont(Some(&NSFont::systemFontOfSize(13.0)));
                version.setTextColor(Some(&NSColor::secondaryLabelColor()));
                let title = hstack(&[&name, &version], mtm);
                title.setSpacing(6.0);
                title.setAlignment(NSLayoutAttribute::FirstBaseline);
                item.setLabel(ns_string!("Robloader"));
                item.setView(Some(&title));
                item.setBordered(false);
            }
            ID_URL => {
                let capsule = UI.with(|c| c.borrow().as_ref().map(|ui| ui.url_capsule.clone()))?;
                item.setLabel(ns_string!("Lien"));
                item.setView(Some(&capsule));
            }
            ID_CHIP => {
                item.setLabel(ns_string!("Source"));
                item.setTitle(ns_string!(""));
                item.setBordered(false);
                set_item_hidden(&item, true);
            }
            ID_PASTE => {
                item.setLabel(ns_string!("Coller"));
                item.setToolTip(Some(ns_string!("Coller le lien")));
                item.setImage(symbol("doc.on.clipboard", "Coller").as_deref());
                item.setBordered(true);
                unsafe {
                    item.setTarget(Some(self.as_target()));
                    item.setAction(Some(sel!(onPaste:)));
                }
            }
            ID_UPDATE => {
                item.setLabel(ns_string!("Mise à jour"));
                item.setTitle(ns_string!("Mise à jour"));
                item.setBordered(true);
                unsafe {
                    item.setTarget(Some(self.as_target()));
                    item.setAction(Some(sel!(onUpdate:)));
                }
                set_item_hidden(&item, true);
            }
            ID_DOWNLOAD => {
                item.setLabel(ns_string!("Télécharger"));
                item.setTitle(ns_string!("Télécharger"));
                item.setImage(symbol("arrow.down", "Télécharger").as_deref());
                item.setBordered(true);
                unsafe {
                    item.setTarget(Some(self.as_target()));
                    item.setAction(Some(sel!(onDownload:)));
                }
                // Bouton principal teinte de l'accent (macOS 26+).
                if responds(&item, sel!(setStyle:)) {
                    item.setStyle(NSToolbarItemStyle::Prominent);
                }
            }
            _ => return None,
        }
        UI.with(|c| {
            if let Some(ui) = c.borrow_mut().as_mut() {
                match id.as_str() {
                    ID_CHIP => ui.chip_item = Some(item.clone()),
                    ID_UPDATE => ui.update_item = Some(item.clone()),
                    _ => {}
                }
            }
        });
        Some(item)
    }

    fn url_focus_changed(&self, notification: &NSNotification, focused: bool) {
        let Some(sender) = notification.object() else { return };
        let ptr = Retained::as_ptr(&sender) as *const AnyObject;
        with_ui(|ui| {
            if ptr == Retained::as_ptr(&ui.url_field).cast() {
                ui.url_capsule.set_focused(focused);
            }
        });
    }

    fn text_changed(&self, sender: Option<&AnyObject>) {
        let Some(sender) = sender else { return };
        let ptr = sender as *const AnyObject;
        enum Which {
            Url(String),
            Start(String),
            End(String),
            None,
        }
        let which = with_ui(|ui| {
            if ptr == Retained::as_ptr(&ui.url_field).cast() {
                Which::Url(ui.url_field.stringValue().to_string())
            } else if ptr == Retained::as_ptr(&ui.start_field).cast() {
                Which::Start(ui.start_field.stringValue().to_string())
            } else if ptr == Retained::as_ptr(&ui.end_field).cast() {
                Which::End(ui.end_field.stringValue().to_string())
            } else {
                Which::None
            }
        })
        .unwrap_or(Which::None);
        match which {
            Which::Url(u) => apply_profile(detect_profile(&u)),
            Which::Start(v) => with_settings(|s| s.start = v.trim().into()),
            Which::End(v) => with_settings(|s| s.end = v.trim().into()),
            Which::None => {}
        }
    }

    fn request_download(&self) {
        let url = with_ui(|ui| ui.url_field.stringValue().to_string()).unwrap_or_default();
        let url = url.trim().to_string();
        let profile = detect_profile(&url);
        let mut settings = with_settings(|s| s.clone());
        if !settings.clip {
            settings.start.clear();
            settings.end.clear();
        }
        settings.subs &= profile.subtitles;
        settings.thumb &= profile.thumbnail;
        let _ = self.ivars().app.emit(
            "mac://download",
            DownloadRequest { url: url.clone(), settings },
        );
        if url.is_empty() {
            focus_url_field();
        } else {
            with_ui(|ui| {
                ui.url_field.setStringValue(ns_string!(""));
                ui.start_field.setStringValue(ns_string!(""));
                ui.end_field.setStringValue(ns_string!(""));
            });
            with_settings(|s| {
                s.start.clear();
                s.end.clear();
            });
            apply_profile(&DEFAULT_PROFILE);
        }
    }
}

fn toolbar_identifiers() -> Retained<NSArray<NSString>> {
    let flexible: &NSString = unsafe { NSToolbarFlexibleSpaceItemIdentifier };
    let space: &NSString = unsafe { NSToolbarSpaceItemIdentifier };
    let separator: &NSString = unsafe { NSToolbarSidebarTrackingSeparatorItemIdentifier };
    let ids = [
        NSString::from_str(ID_TITLE),
        NSString::from_str(&flexible.to_string()),
        NSString::from_str(&separator.to_string()),
        NSString::from_str(ID_URL),
        NSString::from_str(ID_CHIP),
        // Un espace separe Coller de la capsule : sinon la barre les fusionne
        // dans un meme groupe de verre.
        NSString::from_str(&space.to_string()),
        NSString::from_str(ID_PASTE),
        NSString::from_str(ID_UPDATE),
        NSString::from_str(ID_DOWNLOAD),
    ];
    NSArray::from_retained_slice(&ids)
}

// ---------- Aides ----------

fn retain_ref<T: objc2::Message>(obj: &T) -> Retained<T> {
    // Une reference valide pointe sur un objet vivant : retain ne renvoie jamais None.
    unsafe { Retained::retain(obj as *const T as *mut T) }.expect("objet Obj-C valide")
}

fn responds(obj: &NSObject, selector: Sel) -> bool {
    obj.respondsToSelector(selector)
}

// NSToolbarItem.hidden n'existe qu'a partir de macOS 15.
fn set_item_hidden(item: &NSToolbarItem, hidden: bool) {
    if responds(item, sel!(setHidden:)) {
        item.setHidden(hidden);
    }
}

fn symbol(name: &str, description: &str) -> Option<Retained<NSImage>> {
    NSImage::imageWithSystemSymbolName_accessibilityDescription(
        &NSString::from_str(name),
        Some(&NSString::from_str(description)),
    )
}

fn return_key_pressed(mtm: MainThreadMarker) -> bool {
    let app = NSApplication::sharedApplication(mtm);
    match app.currentEvent() {
        Some(ev) => ev.r#type() == NSEventType::KeyDown && matches!(ev.keyCode(), 36 | 76),
        None => false,
    }
}

// Bouton de qualite choisi : teinte de l'accent systeme.
fn select_quality(buttons: &[Retained<NSButton>], idx: usize) {
    let accent = NSColor::controlAccentColor();
    for (i, b) in buttons.iter().enumerate() {
        let on = i == idx;
        b.setBezelColor(if on { Some(&accent) } else { None });
        b.setState(if on { NSControlStateValueOn } else { NSControlStateValueOff });
    }
}

fn fill_outputs(popup: &NSPopUpButton, transcode: bool, selected: &str) {
    let list: &[&str] = if transcode { &OUTPUTS_TRANSCODE } else { &OUTPUTS_NATIVE };
    popup.removeAllItems();
    let titles: Vec<Retained<NSString>> = list.iter().map(|t| NSString::from_str(t)).collect();
    popup.addItemsWithTitles(&NSArray::from_retained_slice(&titles));
    popup.selectItemWithTitle(&NSString::from_str(selected));
}

// Adapte le champ, la pastille et les sections au site detecte.
fn apply_profile(profile: &'static Profile) {
    with_ui(|ui| {
        if ui.profile_id == profile.id {
            return;
        }
        ui.profile_id = profile.id;
        ui.url_field
            .setPlaceholderString(Some(&NSString::from_str(profile.placeholder)));
        if let Some(chip) = &ui.chip_item {
            chip.setTitle(&NSString::from_str(profile.label));
            set_item_hidden(chip, profile.id == "default");
        }
        ui.quality_reveal.set(profile.ladder);
        ui.subs_reveal.set(profile.subtitles);
        ui.thumb_reveal.set(profile.thumbnail);
    });
}

// ---------- Construction ----------

fn label(text: &str, mtm: MainThreadMarker) -> Retained<NSTextField> {
    NSTextField::labelWithString(&NSString::from_str(text), mtm)
}

fn section_header(text: &str, mtm: MainThreadMarker) -> Retained<NSTextField> {
    let l = label(text, mtm);
    l.setFont(Some(&NSFont::systemFontOfSize_weight(11.0, unsafe {
        NSFontWeightSemibold
    })));
    l.setTextColor(Some(&NSColor::secondaryLabelColor()));
    l
}

fn stack_of(views: &[&NSView], mtm: MainThreadMarker) -> Retained<NSStackView> {
    let stack = NSStackView::new(mtm);
    for v in views {
        stack.addArrangedSubview(v);
    }
    stack
}

fn hstack(views: &[&NSView], mtm: MainThreadMarker) -> Retained<NSStackView> {
    let stack = stack_of(views, mtm);
    stack.setOrientation(NSUserInterfaceLayoutOrientation::Horizontal);
    stack.setSpacing(8.0);
    stack.setAlignment(NSLayoutAttribute::CenterY);
    stack
}

fn vstack(views: &[&NSView], spacing: f64, mtm: MainThreadMarker) -> Retained<NSStackView> {
    let stack = stack_of(views, mtm);
    stack.setOrientation(NSUserInterfaceLayoutOrientation::Vertical);
    stack.setAlignment(NSLayoutAttribute::Leading);
    stack.setSpacing(spacing);
    stack
}

fn new_switch(controller: &Controller, on: bool, mtm: MainThreadMarker) -> Retained<NSSwitch> {
    let sw = NSSwitch::new(mtm);
    sw.setState(if on { NSControlStateValueOn } else { NSControlStateValueOff });
    unsafe {
        sw.setTarget(Some(controller.as_target()));
        sw.setAction(Some(sel!(onSwitch:)));
    }
    sw
}

// Ligne « libelle ........ controle ». `label_first` : le libelle ne se
// tronque jamais (c'est le controle qui cede la place).
fn row(text: &str, control: &NSView, label_first: bool, mtm: MainThreadMarker) -> Retained<NSStackView> {
    let l = label(text, mtm);
    l.setLineBreakMode(NSLineBreakMode::ByTruncatingTail);
    l.setContentHuggingPriority_forOrientation(1.0, NSLayoutConstraintOrientation::Horizontal);
    let (label_resist, control_resist) = if label_first { (760.0, 250.0) } else { (250.0, 760.0) };
    l.setContentCompressionResistancePriority_forOrientation(
        label_resist,
        NSLayoutConstraintOrientation::Horizontal,
    );
    control.setContentCompressionResistancePriority_forOrientation(
        control_resist,
        NSLayoutConstraintOrientation::Horizontal,
    );
    let r = hstack(&[&l, control], mtm);
    r.setDistribution(NSStackViewDistribution::Fill);
    r.heightAnchor().constraintGreaterThanOrEqualToConstant(26.0).setActive(true);
    r
}

fn pin_width(view: &NSView, to: &NSView, inset: f64) {
    let leading = view
        .leadingAnchor()
        .constraintEqualToAnchor_constant(&to.leadingAnchor(), inset);
    let trailing = view
        .trailingAnchor()
        .constraintEqualToAnchor_constant(&to.trailingAnchor(), -inset);
    NSLayoutConstraint::activateConstraints(&NSArray::from_retained_slice(&[leading, trailing]));
}

define_class!(
    // Defilement du volet : le contenu s'efface progressivement sous la barre
    // d'outils (AppKit n'expose pas d'API pour l'effet de bord du systeme, et
    // il ne s'applique pas ici). Masque en degrade recale a chaque layout.
    #[unsafe(super(NSScrollView))]
    #[thread_kind = MainThreadOnly]
    #[name = "RobloaderFadingScrollView"]
    struct FadingScrollView;

    impl FadingScrollView {
        #[unsafe(method(layout))]
        fn layout(&self) {
            unsafe {
                let _: () = msg_send![super(self), layout];
            }
            self.update_fade_mask();
        }
    }
);

impl FadingScrollView {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this: Retained<Self> = unsafe { msg_send![Self::alloc(mtm), init] };
        this.setWantsLayer(true);
        this
    }

    fn update_fade_mask(&self) {
        let Some(gradient_class) = AnyClass::get(c"CAGradientLayer") else {
            return;
        };
        let bounds = self.bounds();
        let height = bounds.size.height;
        if height < 1.0 {
            return;
        }
        // Hauteur masquee par la barre d'outils (fenetre - zone de contenu libre).
        let toolbar = self
            .window()
            .map(|w| (w.frame().size.height - w.contentLayoutRect().size.height).max(0.0))
            .unwrap_or(52.0);
        let fade_start = (toolbar * 0.45).min(height);
        let fade_end = (toolbar + 14.0).min(height);
        unsafe {
            let layer: *mut AnyObject = msg_send![self, layer];
            if layer.is_null() {
                return;
            }
            let transaction = AnyClass::get(c"CATransaction");
            if let Some(t) = transaction {
                let _: () = msg_send![t, begin];
                let _: () = msg_send![t, setDisableActions: true];
            }
            let mut mask: *mut AnyObject = msg_send![layer, mask];
            if mask.is_null() {
                mask = msg_send![gradient_class, layer];
                let clear = NSColor::colorWithWhite_alpha(0.0, 0.0);
                let opaque = NSColor::colorWithWhite_alpha(0.0, 1.0);
                let clear_cg: *mut AnyObject = msg_send![&*clear, CGColor];
                let opaque_cg: *mut AnyObject = msg_send![&*opaque, CGColor];
                let colors: *mut AnyObject = msg_send![objc2::class!(NSMutableArray), array];
                for c in [clear_cg, clear_cg, opaque_cg, opaque_cg] {
                    let _: () = msg_send![colors, addObject: c];
                }
                let _: () = msg_send![mask, setColors: colors];
                let _: () = msg_send![layer, setMask: mask];
            }
            let _: () = msg_send![mask, setFrame: bounds];
            // Le haut visuel depend du sens de la couche.
            let flipped: bool = msg_send![layer, contentsAreFlipped];
            let (start, end) = if flipped { (0.0, 1.0) } else { (1.0, 0.0) };
            let _: () = msg_send![mask, setStartPoint: NSPoint::new(0.5, start)];
            let _: () = msg_send![mask, setEndPoint: NSPoint::new(0.5, end)];
            let locations = NSArray::from_retained_slice(&[
                NSNumber::new_f64(0.0),
                NSNumber::new_f64(fade_start / height),
                NSNumber::new_f64(fade_end / height),
                NSNumber::new_f64(1.0),
            ]);
            let _: () = msg_send![mask, setLocations: &*locations];
            if let Some(t) = transaction {
                let _: () = msg_send![t, commit];
            }
        }
    }
}

define_class!(
    // Vue retournee (origine en haut) : le contenu du volet part du haut.
    #[unsafe(super(NSView))]
    #[thread_kind = MainThreadOnly]
    #[name = "RobloaderFlippedView"]
    struct FlippedView;

    impl FlippedView {
        #[unsafe(method(isFlipped))]
        fn is_flipped(&self) -> bool {
            true
        }
    }
);

fn build_ui(
    controller: &Controller,
    window: Retained<NSWindow>,
    mtm: MainThreadMarker,
) -> (Retained<NSView>, Ui) {
    let settings = with_settings(|s| s.clone());
    const SIDE: f64 = 16.0;

    // Qualite : grille 3 colonnes de boutons natifs, le choix teinte d'accent.
    let quality_buttons: Vec<Retained<NSButton>> = QUALITY_SHORT
        .iter()
        .enumerate()
        .map(|(i, q)| {
            let b = unsafe {
                NSButton::buttonWithTitle_target_action(
                    &NSString::from_str(q),
                    Some(controller.as_target()),
                    Some(sel!(onQuality:)),
                    mtm,
                )
            };
            b.setTag(i as isize);
            b.setToolTip(Some(&NSString::from_str(QUALITY_LABELS[i])));
            b
        })
        .collect();
    let idx = QUALITY_LABELS
        .iter()
        .position(|q| *q == settings.quality_label)
        .unwrap_or(0);
    select_quality(&quality_buttons, idx);
    let filler = NSView::new(mtm);
    let grid_row1 = hstack(&[&quality_buttons[0], &quality_buttons[1], &quality_buttons[2]], mtm);
    let grid_row2 = hstack(&[&quality_buttons[3], &quality_buttons[4], &filler], mtm);
    for r in [&grid_row1, &grid_row2] {
        r.setDistribution(NSStackViewDistribution::FillEqually);
        r.setSpacing(6.0);
    }
    let quality_header = section_header("Qualité", mtm);
    let quality_content = vstack(&[&quality_header, &grid_row1, &grid_row2], 6.0, mtm);
    pin_width(&grid_row1, &quality_content, 0.0);
    pin_width(&grid_row2, &quality_content, 0.0);
    quality_content.setCustomSpacing_afterView(8.0, &quality_header);
    let quality_reveal = Reveal::new(&quality_content, 0.0, 22.0, true, mtm);

    // Options.
    let options_header = section_header("Options", mtm);
    let clip_switch = new_switch(controller, settings.clip, mtm);
    let clip_row = row("Extraire un passage", &clip_switch, true, mtm);

    let start_field = NSTextField::textFieldWithString(ns_string!(""), mtm);
    let end_field = NSTextField::textFieldWithString(ns_string!(""), mtm);
    for (f, ph) in [(&start_field, "00:00"), (&end_field, "01:30")] {
        f.setPlaceholderString(Some(&NSString::from_str(ph)));
        unsafe {
            f.setDelegate(Some(ProtocolObject::from_ref(controller)));
            f.setTarget(Some(controller.as_target()));
            f.setAction(Some(sel!(onSubmit:)));
        }
        f.widthAnchor().constraintEqualToConstant(72.0).setActive(true);
    }
    let start_label = label("Début", mtm);
    let end_label = label("Fin", mtm);
    for l in [&start_label, &end_label] {
        l.setTextColor(Some(&NSColor::secondaryLabelColor()));
    }
    let times_row = hstack(&[&start_label, &start_field, &end_label, &end_field], mtm);
    times_row.setCustomSpacing_afterView(14.0, &start_field);
    let times_reveal = Reveal::new(&times_row, 10.0, 0.0, settings.clip, mtm);

    let transcode_switch = new_switch(controller, settings.transcode, mtm);
    let transcode_row = row("Transcodage", &transcode_switch, true, mtm);

    let output_popup = NSPopUpButton::initWithFrame_pullsDown(
        NSPopUpButton::alloc(mtm),
        NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(120.0, 24.0)),
        false,
    );
    fill_outputs(&output_popup, settings.transcode, &settings.output);
    unsafe {
        output_popup.setTarget(Some(controller.as_target()));
        output_popup.setAction(Some(sel!(onOutput:)));
    }
    let output_row = row("Format de sortie", &output_popup, true, mtm);
    let output_reveal = Reveal::new(&output_row, 10.0, 0.0, settings.transcode, mtm);

    let subs_switch = new_switch(controller, settings.subs, mtm);
    let subs_row = row("Sous-titres (.srt)", &subs_switch, true, mtm);
    let subs_reveal = Reveal::new(&subs_row, 10.0, 0.0, true, mtm);
    let thumb_switch = new_switch(controller, settings.thumb, mtm);
    let thumb_row = row("Miniature", &thumb_switch, true, mtm);
    let thumb_reveal = Reveal::new(&thumb_row, 10.0, 0.0, true, mtm);

    // Destination.
    let dest_header = section_header("Destination", mtm);
    let dest_label = label("…", mtm);
    dest_label.setLineBreakMode(NSLineBreakMode::ByTruncatingMiddle);
    dest_label.setContentCompressionResistancePriority_forOrientation(
        250.0,
        NSLayoutConstraintOrientation::Horizontal,
    );
    let choose = unsafe {
        NSButton::buttonWithTitle_target_action(
            ns_string!("Choisir…"),
            Some(controller.as_target()),
            Some(sel!(onChooseDestination:)),
            mtm,
        )
    };
    let show = unsafe {
        NSButton::buttonWithTitle_target_action(
            ns_string!("Afficher"),
            Some(controller.as_target()),
            Some(sel!(onRevealDestination:)),
            mtm,
        )
    };
    let dest_buttons = hstack(&[&choose, &show], mtm);
    dest_buttons.setDistribution(NSStackViewDistribution::FillEqually);

    // Pied : etat des cookies / Deno.
    let foot_label = label("", mtm);
    foot_label.setFont(Some(&NSFont::systemFontOfSize(11.0)));
    foot_label.setTextColor(Some(&NSColor::secondaryLabelColor()));
    foot_label.setLineBreakMode(NSLineBreakMode::ByWordWrapping);
    foot_label.setContentCompressionResistancePriority_forOrientation(
        250.0,
        NSLayoutConstraintOrientation::Horizontal,
    );
    let repair_button = unsafe {
        NSButton::buttonWithTitle_target_action(
            ns_string!("Réparer les cookies…"),
            Some(controller.as_target()),
            Some(sel!(onRepair:)),
            mtm,
        )
    };
    repair_button.setHidden(true);

    let stack = vstack(
        &[
            &quality_reveal.container,
            &options_header,
            &clip_row,
            &times_reveal.container,
            &transcode_row,
            &output_reveal.container,
            &subs_reveal.container,
            &thumb_reveal.container,
            &dest_header,
            &dest_label,
            &dest_buttons,
            &foot_label,
            &repair_button,
        ],
        10.0,
        mtm,
    );
    stack.setEdgeInsets(NSEdgeInsets { top: 10.0, left: SIDE, bottom: SIDE, right: SIDE });
    // Les marges des zones revelables sont dans leur conteneur.
    for v in [
        &*quality_reveal.container,
        &clip_row,
        &transcode_row,
        &output_reveal.container,
        &subs_reveal.container,
    ] {
        stack.setCustomSpacing_afterView(0.0, v);
    }
    stack.setCustomSpacing_afterView(22.0, &thumb_reveal.container);
    stack.setCustomSpacing_afterView(22.0, &dest_buttons);
    stack.setTranslatesAutoresizingMaskIntoConstraints(false);

    for v in [
        &*quality_reveal.container,
        &clip_row,
        &times_reveal.container,
        &transcode_row,
        &output_reveal.container,
        &subs_reveal.container,
        &thumb_reveal.container,
        &dest_label,
        &dest_buttons,
        &foot_label,
    ] {
        pin_width(v, &stack, SIDE);
    }

    // Defilement si la fenetre est basse (les marges sous la barre d'outils
    // sont gerees par automaticallyAdjustsContentInsets).
    let document: Retained<FlippedView> = unsafe { msg_send![FlippedView::alloc(mtm), init] };
    document.setTranslatesAutoresizingMaskIntoConstraints(false);
    document.addSubview(&stack);
    let scroll = FadingScrollView::new(mtm);
    scroll.setDrawsBackground(false);
    scroll.setHasVerticalScroller(true);
    scroll.setAutohidesScrollers(true);
    scroll.setDocumentView(Some(&document));
    let clip_view = scroll.contentView();
    let constraints = [
        stack.topAnchor().constraintEqualToAnchor(&document.topAnchor()),
        stack.leadingAnchor().constraintEqualToAnchor(&document.leadingAnchor()),
        stack.trailingAnchor().constraintEqualToAnchor(&document.trailingAnchor()),
        stack.bottomAnchor().constraintEqualToAnchor(&document.bottomAnchor()),
        document.topAnchor().constraintEqualToAnchor(&clip_view.topAnchor()),
        document.leadingAnchor().constraintEqualToAnchor(&clip_view.leadingAnchor()),
        document.trailingAnchor().constraintEqualToAnchor(&clip_view.trailingAnchor()),
    ];
    NSLayoutConstraint::activateConstraints(&NSArray::from_retained_slice(&constraints));

    // Capsule du lien : icone + champ sans bordure, dessin facon Safari.
    let url_field = NSTextField::textFieldWithString(ns_string!(""), mtm);
    url_field.setBordered(false);
    url_field.setBezeled(false);
    url_field.setDrawsBackground(false);
    url_field.setFocusRingType(NSFocusRingType::None);
    url_field.setUsesSingleLineMode(true);
    url_field.setLineBreakMode(NSLineBreakMode::ByTruncatingTail);
    url_field.setFont(Some(&NSFont::systemFontOfSize(13.0)));
    url_field.setPlaceholderString(Some(&NSString::from_str(DEFAULT_PROFILE.placeholder)));
    unsafe {
        url_field.setDelegate(Some(ProtocolObject::from_ref(controller)));
        url_field.setTarget(Some(controller.as_target()));
        url_field.setAction(Some(sel!(onSubmit:)));
    }
    let url_capsule = UrlCapsule::new(&url_field, mtm);
    let icon = match symbol("link", "Lien") {
        Some(img) => NSImageView::imageViewWithImage(&img, mtm),
        None => NSImageView::new(mtm),
    };
    icon.setContentTintColor(Some(&NSColor::secondaryLabelColor()));
    for v in [&*icon as &NSView, &url_field] {
        v.setTranslatesAutoresizingMaskIntoConstraints(false);
        url_capsule.addSubview(v);
    }
    let capsule_constraints = [
        url_capsule.heightAnchor().constraintEqualToConstant(36.0),
        url_capsule.widthAnchor().constraintGreaterThanOrEqualToConstant(260.0),
        icon.leadingAnchor().constraintEqualToAnchor_constant(&url_capsule.leadingAnchor(), 13.0),
        icon.centerYAnchor().constraintEqualToAnchor(&url_capsule.centerYAnchor()),
        url_field.leadingAnchor().constraintEqualToAnchor_constant(&icon.trailingAnchor(), 8.0),
        url_field.trailingAnchor().constraintEqualToAnchor_constant(&url_capsule.trailingAnchor(), -14.0),
        url_field.centerYAnchor().constraintEqualToAnchor(&url_capsule.centerYAnchor()),
    ];
    NSLayoutConstraint::activateConstraints(&NSArray::from_retained_slice(&capsule_constraints));
    // Pas de plafond reel : la capsule absorbe toute la largeur libre, pour que
    // Coller / Mise a jour / Telecharger restent colles a droite (plein ecran compris).
    let max = url_capsule.widthAnchor().constraintLessThanOrEqualToConstant(100_000.0);
    max.setActive(true);

    let ui = Ui {
        window,
        url_capsule,
        url_field,
        chip_item: None,
        update_item: None,
        quality_reveal,
        quality_buttons,
        clip_switch,
        times_reveal,
        start_field,
        end_field,
        transcode_switch,
        output_reveal,
        output_popup,
        subs_reveal,
        subs_switch,
        thumb_reveal,
        thumb_switch,
        dest_label,
        foot_label,
        repair_button,
        profile_id: DEFAULT_PROFILE.id,
    };
    (Retained::into_super(Retained::into_super(scroll)), ui)
}

/// Installe barre d'outils + volet natifs. A appeler dans setup (thread principal).
pub fn install(app: &AppHandle, window: &tauri::WebviewWindow) -> bool {
    let Some(mtm) = MainThreadMarker::new() else {
        return false;
    };
    let Ok(ns_window) = window.ns_window() else {
        return false;
    };
    let ns_window: Retained<NSWindow> =
        match unsafe { Retained::retain(ns_window as *mut NSWindow) } {
            Some(w) => w,
            None => return false,
        };

    let version = app.package_info().version.to_string();
    let controller = Controller::new(app.clone(), version, mtm);
    let (sidebar_view, ui) = build_ui(&controller, ns_window.clone(), mtm);

    // Volet systeme + webview en panneau de contenu.
    let Some(web_container) = ns_window.contentView() else {
        return false;
    };
    let sidebar_vc = NSViewController::new(mtm);
    sidebar_vc.setView(&sidebar_view);
    let sidebar_item = NSSplitViewItem::splitViewItemWithViewController(&sidebar_vc);
    if responds(&sidebar_item, sel!(setBehavior:)) {
        let _: () = unsafe { msg_send![&*sidebar_item, setBehavior: NSSplitViewItemBehavior::Sidebar] };
    }
    sidebar_item.setCanCollapse(false);
    sidebar_item.setMinimumThickness(290.0);
    sidebar_item.setMaximumThickness(290.0);

    let content_vc = NSViewController::new(mtm);
    content_vc.setView(&web_container);
    let content_item = NSSplitViewItem::splitViewItemWithViewController(&content_vc);

    let split = NSSplitViewController::new(mtm);
    split.addSplitViewItem(&sidebar_item);
    split.addSplitViewItem(&content_item);

    // Pendant l'echange, AppKit redimensionne la fenetre alors que contentView
    // est momentanement nil : le delegue de tao (windowDidResize) fait alors
    // contentView().unwrap() et l'app avorte. On le detache le temps de l'echange.
    let frame = ns_window.frame();
    let tao_delegate = ns_window.delegate();
    ns_window.setDelegate(None);
    ns_window.setContentViewController(Some(&split));
    ns_window.setFrame_display(frame, true);
    ns_window.setDelegate(tao_delegate.as_deref());

    UI.with(|c| *c.borrow_mut() = Some(ui));
    CONTROLLER.with(|c| *c.borrow_mut() = Some(controller.clone()));

    // Barre d'outils unifiee ; le titre est un item au-dessus du volet.
    let toolbar = NSToolbar::initWithIdentifier(NSToolbar::alloc(mtm), ns_string!("RobloaderToolbar"));
    toolbar.setDelegate(Some(ProtocolObject::from_ref(&*controller)));
    toolbar.setAllowsUserCustomization(false);
    toolbar.setDisplayMode(NSToolbarDisplayMode::IconOnly);
    ns_window.setToolbar(Some(&toolbar));
    ns_window.setToolbarStyle(NSWindowToolbarStyle::Unified);
    ns_window.setTitleVisibility(NSWindowTitleVisibility::Hidden);
    INSTALLED.store(true, Ordering::Relaxed);
    true
}

// ---------- Fonctions appelees par les commandes du front (lib.rs) ----------

fn on_main(app: &AppHandle, f: impl FnOnce() + Send + 'static) {
    let _ = app.run_on_main_thread(f);
}

pub fn native_ui_installed() -> bool {
    INSTALLED.load(Ordering::Relaxed)
}

pub fn set_env(
    app: AppHandle,
    download_dir: String,
    cookies_ok: bool,
    cookies_source: String,
    js_runtime: bool,
) {
    on_main(&app, move || {
        with_ui(|ui| {
            let pretty = match std::env::var("HOME") {
                Ok(home) if download_dir.starts_with(&home) => {
                    format!("~{}", &download_dir[home.len()..])
                }
                _ => download_dir.clone(),
            };
            ui.dest_label.setStringValue(&NSString::from_str(&pretty));
            ui.dest_label.setToolTip(Some(&NSString::from_str(&download_dir)));
            let mut foot = if cookies_ok {
                format!("Cookies {cookies_source} ✓")
            } else {
                "Cookies absents".to_string()
            };
            if !js_runtime {
                foot.push_str(" · 4K limitée (Deno absent)");
            }
            ui.foot_label.setStringValue(&NSString::from_str(&foot));
            ui.repair_button.setHidden(cookies_ok);
        });
    });
}

pub fn set_update(app: AppHandle, version: Option<String>, installing: bool) {
    on_main(&app, move || {
        with_ui(|ui| {
            if let Some(item) = &ui.update_item {
                set_item_hidden(item, version.is_none());
                item.setEnabled(!installing);
                let title = if installing { "Installation…" } else { "Mise à jour" };
                item.setTitle(&NSString::from_str(title));
                if let Some(v) = &version {
                    item.setToolTip(Some(&NSString::from_str(&format!(
                        "Installer la version {v} et relancer"
                    ))));
                }
            }
        });
    });
}

/// Hauteur de la barre d'outils (le contenu de la webview passe dessous).
pub fn toolbar_height(app: AppHandle) -> f64 {
    let (tx, rx) = std::sync::mpsc::channel();
    on_main(&app, move || {
        let h = with_ui(|ui| {
            let frame = ui.window.frame();
            let layout = ui.window.contentLayoutRect();
            (frame.size.height - layout.size.height).max(0.0)
        })
        .unwrap_or(52.0);
        let _ = tx.send(h);
    });
    rx.recv().unwrap_or(52.0)
}
