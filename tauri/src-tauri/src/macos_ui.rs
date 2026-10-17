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
use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject, ProtocolObject, Sel};
use objc2::{define_class, msg_send, sel, DefinedClass, MainThreadOnly};
use objc2_app_kit::{
    NSAlert, NSApplication, NSBackingStoreType, NSBezelStyle, NSBox, NSBoxType, NSButton, NSCellImagePosition, NSColor, NSControlStateValueOff,
    NSControlStateValueOn, NSControlTextEditingDelegate, NSEvent, NSEventType, NSFocusRingType,
    NSFont, NSFontWeightBold, NSFontWeightRegular, NSImageSymbolConfiguration, NSImageSymbolScale, NSFontWeightSemibold, NSImage, NSImageView, NSLayoutAttribute,
    NSLayoutConstraint, NSLayoutConstraintOrientation, NSLineBreakMode, NSPasteboard,
    NSPasteboardTypeString, NSPopUpButton, NSScrollView, NSSegmentSwitchTracking, NSSegmentedControl,
    NSSplitViewController, NSSplitViewItem,
    NSSplitViewItemBehavior, NSStackView, NSStackViewDistribution, NSSwitch, NSTextField,
    NSTextFieldDelegate, NSToolbar, NSToolbarDelegate, NSToolbarDisplayMode,
    NSToolbarFlexibleSpaceItemIdentifier, NSToolbarItem, NSToolbarSpaceItemIdentifier, NSToolbarItemStyle,
    NSToolbarSidebarTrackingSeparatorItemIdentifier, NSUserInterfaceLayoutOrientation, NSView,
    NSViewController, NSWindow, NSWindowStyleMask, NSWindowTitleVisibility, NSWindowToolbarStyle,
};
use objc2_foundation::{
    ns_string, MainThreadMarker, NSArray, NSEdgeInsets, NSNotification, NSObject,
    NSNumber, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString, NSTimer,
};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::i18n::{self, tr};

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
    label_en: &'static str,
    domains: &'static [&'static str],
    placeholder: &'static str,
    placeholder_en: &'static str,
    ladder: bool,
    subtitles: bool,
    thumbnail: bool,
}

const DEFAULT_PROFILE: Profile = Profile {
    id: "default",
    label: "Vidéo",
    label_en: "Video",
    domains: &[],
    placeholder: "Colle un lien (YouTube, TikTok, Instagram, X, Weibo)…",
    placeholder_en: "Paste a link (YouTube, TikTok, Instagram, X, Weibo)…",
    ladder: true,
    subtitles: true,
    thumbnail: true,
};
const SITE_PROFILES: [Profile; 5] = [
    Profile { id: "youtube", label: "YouTube", label_en: "YouTube", domains: &["youtube.com", "youtu.be"], placeholder: "Colle un lien YouTube ici…", placeholder_en: "Paste a YouTube link here…", ladder: true, subtitles: true, thumbnail: true },
    Profile { id: "tiktok", label: "TikTok", label_en: "TikTok", domains: &["tiktok.com"], placeholder: "Colle un lien TikTok ici…", placeholder_en: "Paste a TikTok link here…", ladder: false, subtitles: false, thumbnail: true },
    Profile { id: "instagram", label: "Instagram", label_en: "Instagram", domains: &["instagram.com", "instagr.am"], placeholder: "Colle un lien Instagram ici…", placeholder_en: "Paste an Instagram link here…", ladder: false, subtitles: false, thumbnail: false },
    Profile { id: "x", label: "X", label_en: "X", domains: &["twitter.com", "x.com"], placeholder: "Colle un lien X (Twitter) ici…", placeholder_en: "Paste an X (Twitter) link here…", ladder: false, subtitles: false, thumbnail: false },
    Profile { id: "weibo", label: "Weibo", label_en: "Weibo", domains: &["weibo.com", "weibo.cn"], placeholder: "Colle un lien Weibo ici…", placeholder_en: "Paste a Weibo link here…", ladder: false, subtitles: false, thumbnail: false },
];

// Les libelles de qualite et de format servent aussi de valeurs envoyees au
// moteur (en francais) : on ne traduit que l'affichage.
fn quality_display(i: usize) -> &'static str {
    if i == 0 {
        tr(QUALITY_LABELS[0], "Max quality (up to 4K)")
    } else {
        QUALITY_LABELS[i]
    }
}

fn output_display(key: &str) -> &'static str {
    match key {
        "Vidéo (natif)" => tr("Vidéo (natif)", "Video (native)"),
        "Sous-titres seuls (.srt)" => tr("Sous-titres seuls (.srt)", "Subtitles only (.srt)"),
        "HEVC" => "HEVC",
        "ProRes" => "ProRes",
        _ => "Audio WAV",
    }
}

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

#[derive(Clone)]
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

#[derive(Clone)]
struct Ui {
    window: Retained<NSWindow>,
    url_capsule: Retained<UrlCapsule>,
    update_button: Retained<NSButton>,
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
}

thread_local! {
    static UI: RefCell<Option<Ui>> = const { RefCell::new(None) };
    static CONTROLLER: RefCell<Option<Retained<Controller>>> = const { RefCell::new(None) };
    static PROFILE_ID: Cell<&'static str> = const { Cell::new("default") };
}

// Les appels AppKit (vider un champ en edition, donner le focus, masquer un
// item…) peuvent rappeler un delegue de facon synchrone, qui relit UI. On
// travaille donc sur une copie des references (simples retain) : aucun emprunt
// n'est tenu pendant ces appels. Une panique dans un callback Obj-C remonte
// jusqu'au sendEvent: de tao, qui ne peut pas la propager et avorte l'app.
fn with_ui<T>(f: impl FnOnce(&Ui) -> T) -> Option<T> {
    let ui = UI.with(|cell| cell.try_borrow().ok()?.clone())?;
    Some(f(&ui))
}

// Modification de l'etat garde : jamais d'appel AppKit dans `f`.
fn update_ui(f: impl FnOnce(&mut Ui)) {
    UI.with(|cell| {
        if let Ok(mut guard) = cell.try_borrow_mut() {
            if let Some(ui) = guard.as_mut() {
                f(ui);
            }
        }
    });
}

fn focus_url_field() {
    with_ui(|ui| ui.window.makeFirstResponder(Some(&ui.url_field)));
}

// ---------- Changement de langue a chaud ----------
//
// Chaque texte traduisible s'inscrit ici a sa creation ; `relabel` les
// reecrit tous dans la nouvelle langue (appele par i18n::apply). Les textes
// calcules (pied du volet, Mise a jour, formats…) sont recalcules a part.

thread_local! {
    static RELABEL: RefCell<Vec<Box<dyn Fn()>>> = const { RefCell::new(Vec::new()) };
    static LAST_ENV: RefCell<Option<(String, bool, String, bool)>> = const { RefCell::new(None) };
    static LAST_UPDATE: RefCell<(Option<String>, bool)> = const { RefCell::new((None, false)) };
}

fn on_relabel(f: impl Fn() + 'static) {
    RELABEL.with(|r| r.borrow_mut().push(Box::new(f)));
}

pub fn relabel() {
    // Aucun emprunt tenu pendant les appels AppKit (cf. with_ui).
    let list = RELABEL.with(|r| std::mem::take(&mut *r.borrow_mut()));
    for f in &list {
        f();
    }
    RELABEL.with(|r| {
        let mut g = r.borrow_mut();
        let added = std::mem::take(&mut *g);
        *g = list;
        g.extend(added);
    });
    with_ui(|ui| {
        for (i, b) in ui.quality_buttons.iter().enumerate() {
            b.setToolTip(Some(&NSString::from_str(quality_display(i))));
        }
        let (transcode, output) = with_settings(|s| (s.transcode, s.output.clone()));
        fill_outputs(&ui.output_popup, transcode, &output);
    });
    let id = PROFILE_ID.with(|p| p.replace(""));
    apply_profile(profile_by_id(id));
    if let Some((dir, ok, source, js)) = LAST_ENV.with(|e| e.borrow().clone()) {
        show_env(&dir, ok, &source, js);
    }
    let (version, installing) = LAST_UPDATE.with(|u| u.borrow().clone());
    show_update(version, installing);
}

// Libelle traduisible.
fn tlabel(fr: &'static str, en: &'static str, mtm: MainThreadMarker) -> Retained<NSTextField> {
    let l = label(tr(fr, en), mtm);
    let c = l.clone();
    on_relabel(move || c.setStringValue(&NSString::from_str(tr(fr, en))));
    l
}

// Bouton traduisible.
fn tbutton(
    fr: &'static str,
    en: &'static str,
    controller: &Controller,
    action: Sel,
    mtm: MainThreadMarker,
) -> Retained<NSButton> {
    let b = unsafe {
        NSButton::buttonWithTitle_target_action(
            &NSString::from_str(tr(fr, en)),
            Some(controller.as_target()),
            Some(action),
            mtm,
        )
    };
    let c = b.clone();
    on_relabel(move || c.setTitle(&NSString::from_str(tr(fr, en))));
    b
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

// ---------- Loupiote du bouton « Mise a jour » ----------

thread_local! {
    static BLINK_TIMER: RefCell<Option<Retained<NSTimer>>> = const { RefCell::new(None) };
    static BLINK_ON: Cell<bool> = const { Cell::new(true) };
}

// Teinte d'un item de barre d'outils : macOS 26+ seulement.
fn update_tintable(item: &NSToolbarItem) -> bool {
    responds(item, sel!(setBackgroundTintColor:))
}

// Point plein (symbole SF « circle.fill »), allume ou attenue.
fn led_image(on: bool, white: bool) -> Option<Retained<NSImage>> {
    let base = symbol("circle.fill", tr("Nouvelle version disponible", "New version available"))?;
    let color = if white { NSColor::whiteColor() } else { NSColor::systemGreenColor() };
    let color = if on { color } else { color.colorWithAlphaComponent(0.25) };
    let size = NSImageSymbolConfiguration::configurationWithPointSize_weight_scale(
        7.0,
        unsafe { NSFontWeightRegular },
        NSImageSymbolScale::Small,
    );
    let palette = NSImageSymbolConfiguration::configurationWithPaletteColors(
        &NSArray::from_retained_slice(&[color]),
    );
    base.imageWithSymbolConfiguration(&size.configurationByApplyingConfiguration(&palette))
}

fn set_led(on: bool) {
    with_ui(|ui| {
        let Some(item) = &ui.update_item else { return };
        if update_tintable(item) {
            item.setImage(led_image(on, true).as_deref());
        } else if let Some(img) = led_image(on, false) {
            ui.update_button.setImage(Some(&img));
        }
    });
}

fn reduce_motion() -> bool {
    let Some(cls) = AnyClass::get(c"NSWorkspace") else { return false };
    unsafe {
        let ws: *mut AnyObject = msg_send![cls, sharedWorkspace];
        msg_send![ws, accessibilityDisplayShouldReduceMotion]
    }
}

// Clignotement en alternant deux images (les icones d'une barre d'outils ne
// s'animent pas). Fixe si « Reduire les animations » est actif.
fn start_blink() {
    if BLINK_TIMER.with(|t| t.borrow().is_some()) {
        return;
    }
    BLINK_ON.with(|b| b.set(true));
    set_led(true);
    if reduce_motion() {
        return;
    }
    let block = RcBlock::new(|_timer: NonNull<NSTimer>| {
        let on = !BLINK_ON.with(|b| b.get());
        BLINK_ON.with(|b| b.set(on));
        set_led(on);
    });
    let timer = unsafe { NSTimer::scheduledTimerWithTimeInterval_repeats_block(0.8, true, &block) };
    BLINK_TIMER.with(|t| *t.borrow_mut() = Some(timer));
}

fn stop_blink() {
    if let Some(timer) = BLINK_TIMER.with(|t| t.borrow_mut().take()) {
        timer.invalidate();
    }
    BLINK_ON.with(|b| b.set(true));
    set_led(true);
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
            // Les titres sont traduits : la valeur vient de la position.
            let Ok(idx) = usize::try_from(popup.indexOfSelectedItem()) else { return };
            with_settings(|s| {
                let list: &[&str] = if s.transcode { &OUTPUTS_TRANSCODE } else { &OUTPUTS_NATIVE };
                if let Some(key) = list.get(idx) {
                    s.output = (*key).into();
                }
            });
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

        #[unsafe(method(onSettings:))]
        fn on_settings(&self, _sender: Option<&AnyObject>) {
            show_settings_window();
        }

        // Choix Francais / English (Reglages et 1er lancement).
        #[unsafe(method(onLanguage:))]
        fn on_language(&self, sender: Option<&AnyObject>) {
            let Some(seg) = sender.and_then(|s| s.downcast_ref::<NSSegmentedControl>()) else {
                return;
            };
            let lang = if seg.selectedSegment() == 1 { "en" } else { "fr" };
            i18n::apply(&self.ivars().app, lang);
        }

        #[unsafe(method(onOpenSource:))]
        fn on_open_source(&self, _sender: Option<&AnyObject>) {
            let _ = std::process::Command::new("open").arg(SOURCE_URL).spawn();
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
                let capsule = with_ui(|ui| ui.url_capsule.clone())?;
                item.setView(Some(&capsule));
            }
            ID_CHIP => {
                item.setTitle(ns_string!(""));
                item.setBordered(false);
                set_item_hidden(&item, true);
            }
            ID_PASTE => {
                item.setImage(symbol("doc.on.clipboard", tr("Coller", "Paste")).as_deref());
                item.setBordered(true);
                unsafe {
                    item.setTarget(Some(self.as_target()));
                    item.setAction(Some(sel!(onPaste:)));
                }
            }
            ID_UPDATE => {
                // Vert et loupiote avec les elements d'Apple uniquement.
                // macOS 26+ : item proeminent teinte en vert, loupiote blanche.
                // Avant : l'item ne se teinte pas -> vrai bouton de barre
                // d'outils, loupiote verte.
                if update_tintable(&item) {
                    item.setTitle(&NSString::from_str(tr("Mise à jour", "Update")));
                    item.setImage(led_image(true, true).as_deref());
                    item.setBordered(true);
                    unsafe {
                        item.setTarget(Some(self.as_target()));
                        item.setAction(Some(sel!(onUpdate:)));
                    }
                    if responds(&item, sel!(setStyle:)) {
                        item.setStyle(NSToolbarItemStyle::Prominent);
                    }
                    item.setBackgroundTintColor(Some(&NSColor::systemGreenColor()));
                } else {
                    let button = with_ui(|ui| ui.update_button.clone())?;
                    item.setView(Some(&button));
                }
                set_item_hidden(&item, true);
            }
            ID_DOWNLOAD => {
                item.setImage(symbol("arrow.down", tr("Télécharger", "Download")).as_deref());
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
        label_item(&item, &id);
        {
            let (item, id) = (item.clone(), id.clone());
            on_relabel(move || label_item(&item, &id));
        }
        update_ui(|ui| match id.as_str() {
            ID_CHIP => ui.chip_item = Some(item.clone()),
            ID_UPDATE => ui.update_item = Some(item.clone()),
            _ => {}
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

// Textes fixes des items de barre d'outils (le titre de « Mise a jour » suit
// son etat, cf. show_update).
fn label_item(item: &NSToolbarItem, id: &str) {
    let set_label = |fr, en| item.setLabel(&NSString::from_str(tr(fr, en)));
    match id {
        ID_URL => set_label("Lien", "Link"),
        ID_CHIP => set_label("Source", "Source"),
        ID_PASTE => {
            set_label("Coller", "Paste");
            item.setToolTip(Some(&NSString::from_str(tr("Coller le lien", "Paste the link"))));
        }
        ID_UPDATE => set_label("Mise à jour", "Update"),
        ID_DOWNLOAD => {
            set_label("Télécharger", "Download");
            item.setTitle(&NSString::from_str(tr("Télécharger", "Download")));
        }
        _ => {}
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
    let titles: Vec<Retained<NSString>> =
        list.iter().map(|t| NSString::from_str(output_display(t))).collect();
    popup.addItemsWithTitles(&NSArray::from_retained_slice(&titles));
    let idx = list.iter().position(|t| *t == selected).unwrap_or(0);
    popup.selectItemAtIndex(idx as isize);
}

fn profile_by_id(id: &str) -> &'static Profile {
    SITE_PROFILES.iter().find(|p| p.id == id).unwrap_or(&DEFAULT_PROFILE)
}

// Adapte le champ, la pastille et les sections au site detecte.
fn apply_profile(profile: &'static Profile) {
    if PROFILE_ID.with(|p| p.replace(profile.id)) == profile.id {
        return;
    }
    with_ui(|ui| {
        let (placeholder, chip_label) = if i18n::en() {
            (profile.placeholder_en, profile.label_en)
        } else {
            (profile.placeholder, profile.label)
        };
        ui.url_field
            .setPlaceholderString(Some(&NSString::from_str(placeholder)));
        if let Some(chip) = &ui.chip_item {
            chip.setTitle(&NSString::from_str(chip_label));
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

fn section_header(fr: &'static str, en: &'static str, mtm: MainThreadMarker) -> Retained<NSTextField> {
    let l = tlabel(fr, en, mtm);
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
fn row(
    fr: &'static str,
    en: &'static str,
    control: &NSView,
    label_first: bool,
    mtm: MainThreadMarker,
) -> Retained<NSStackView> {
    let l = tlabel(fr, en, mtm);
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
            b.setToolTip(Some(&NSString::from_str(quality_display(i))));
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
    let quality_header = section_header("Qualité", "Quality", mtm);
    let quality_content = vstack(&[&quality_header, &grid_row1, &grid_row2], 6.0, mtm);
    pin_width(&grid_row1, &quality_content, 0.0);
    pin_width(&grid_row2, &quality_content, 0.0);
    quality_content.setCustomSpacing_afterView(8.0, &quality_header);
    let quality_reveal = Reveal::new(&quality_content, 0.0, 22.0, true, mtm);

    // Options.
    let options_header = section_header("Options", "Options", mtm);
    let clip_switch = new_switch(controller, settings.clip, mtm);
    let clip_row = row("Extraire un passage", "Extract a clip", &clip_switch, true, mtm);

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
    let start_label = tlabel("Début", "Start", mtm);
    let end_label = tlabel("Fin", "End", mtm);
    for l in [&start_label, &end_label] {
        l.setTextColor(Some(&NSColor::secondaryLabelColor()));
    }
    let times_row = hstack(&[&start_label, &start_field, &end_label, &end_field], mtm);
    times_row.setCustomSpacing_afterView(14.0, &start_field);
    let times_reveal = Reveal::new(&times_row, 10.0, 0.0, settings.clip, mtm);

    let transcode_switch = new_switch(controller, settings.transcode, mtm);
    let transcode_row = row("Transcodage", "Transcoding", &transcode_switch, true, mtm);

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
    let output_row = row("Format de sortie", "Output format", &output_popup, true, mtm);
    let output_reveal = Reveal::new(&output_row, 10.0, 0.0, settings.transcode, mtm);

    let subs_switch = new_switch(controller, settings.subs, mtm);
    let subs_row = row("Sous-titres (.srt)", "Subtitles (.srt)", &subs_switch, true, mtm);
    let subs_reveal = Reveal::new(&subs_row, 10.0, 0.0, true, mtm);
    let thumb_switch = new_switch(controller, settings.thumb, mtm);
    let thumb_row = row("Miniature", "Thumbnail", &thumb_switch, true, mtm);
    let thumb_reveal = Reveal::new(&thumb_row, 10.0, 0.0, true, mtm);

    // Destination.
    let dest_header = section_header("Destination", "Destination", mtm);
    let dest_label = label("…", mtm);
    dest_label.setLineBreakMode(NSLineBreakMode::ByTruncatingMiddle);
    dest_label.setContentCompressionResistancePriority_forOrientation(
        250.0,
        NSLayoutConstraintOrientation::Horizontal,
    );
    let choose = tbutton("Choisir…", "Choose…", controller, sel!(onChooseDestination:), mtm);
    let show = tbutton("Afficher", "Show", controller, sel!(onRevealDestination:), mtm);
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
    foot_label.setContentHuggingPriority_forOrientation(1.0, NSLayoutConstraintOrientation::Horizontal);
    // Roue des Reglages, a droite de l'etat (meme fenetre que Robloader > Reglages…).
    let gear = unsafe {
        NSButton::buttonWithImage_target_action(
            &symbol("gearshape", tr("Réglages", "Settings")).unwrap_or_else(NSImage::new),
            Some(controller.as_target()),
            Some(sel!(onSettings:)),
            mtm,
        )
    };
    gear.setBordered(false);
    gear.setContentTintColor(Some(&NSColor::secondaryLabelColor()));
    gear.setToolTip(Some(&NSString::from_str(tr("Réglages", "Settings"))));
    {
        let g = gear.clone();
        on_relabel(move || g.setToolTip(Some(&NSString::from_str(tr("Réglages", "Settings")))));
    }
    let foot_row = hstack(&[&foot_label, &gear], mtm);
    let repair_button = tbutton("Réparer les cookies…", "Repair cookies…", controller, sel!(onRepair:), mtm);
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
            &foot_row,
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
        &foot_row,
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
    url_field.setPlaceholderString(Some(&NSString::from_str(tr(
        DEFAULT_PROFILE.placeholder,
        DEFAULT_PROFILE.placeholder_en,
    ))));
    unsafe {
        url_field.setDelegate(Some(ProtocolObject::from_ref(controller)));
        url_field.setTarget(Some(controller.as_target()));
        url_field.setAction(Some(sel!(onSubmit:)));
    }
    let url_capsule = UrlCapsule::new(&url_field, mtm);
    // Mise a jour avant macOS 26 : vrai bouton de barre d'outils (vue de l'item).
    let update_button = unsafe {
        NSButton::buttonWithTitle_image_target_action(
            &NSString::from_str(tr("Mise à jour", "Update")),
            &led_image(true, false).unwrap_or_else(NSImage::new),
            Some(controller.as_target()),
            Some(sel!(onUpdate:)),
            mtm,
        )
    };
    update_button.setBezelStyle(NSBezelStyle::Toolbar);
    update_button.setImagePosition(NSCellImagePosition::ImageLeading);
    let icon = match symbol("link", tr("Lien", "Link")) {
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
        update_button,
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
        LAST_ENV.with(|e| {
            *e.borrow_mut() = Some((download_dir.clone(), cookies_ok, cookies_source.clone(), js_runtime))
        });
        show_env(&download_dir, cookies_ok, &cookies_source, js_runtime);
    });
}

fn show_env(download_dir: &str, cookies_ok: bool, cookies_source: &str, js_runtime: bool) {
    with_ui(|ui| {
        let pretty = match std::env::var("HOME") {
            Ok(home) if download_dir.starts_with(&home) => {
                format!("~{}", &download_dir[home.len()..])
            }
            _ => download_dir.to_string(),
        };
        ui.dest_label.setStringValue(&NSString::from_str(&pretty));
        ui.dest_label.setToolTip(Some(&NSString::from_str(download_dir)));
        let mut foot = if cookies_ok {
            format!("Cookies {cookies_source} ✓")
        } else {
            tr("Cookies absents", "No cookies").to_string()
        };
        if !js_runtime {
            foot.push_str(tr(" · 4K limitée (Deno absent)", " · 4K limited (Deno missing)"));
        }
        ui.foot_label.setStringValue(&NSString::from_str(&foot));
        ui.repair_button.setHidden(cookies_ok);
    });
}

pub fn set_update(app: AppHandle, version: Option<String>, installing: bool) {
    on_main(&app, move || {
        LAST_UPDATE.with(|u| *u.borrow_mut() = (version.clone(), installing));
        show_update(version, installing);
    });
}

fn show_update(version: Option<String>, installing: bool) {
    with_ui(|ui| {
        let Some(item) = &ui.update_item else { return };
        let title = NSString::from_str(if installing {
            tr("Installation…", "Installing…")
        } else {
            tr("Mise à jour", "Update")
        });
        let tip = version.as_ref().map(|v| {
            NSString::from_str(&if i18n::en() {
                format!("Install version {v} and restart")
            } else {
                format!("Installer la version {v} et relancer")
            })
        });
        if update_tintable(item) {
            item.setEnabled(!installing);
            item.setTitle(&title);
            item.setToolTip(tip.as_deref());
        } else {
            ui.update_button.setEnabled(!installing);
            ui.update_button.setTitle(&title);
            ui.update_button.setToolTip(tip.as_deref());
        }
        set_item_hidden(item, version.is_none());
        if version.is_some() && !installing {
            start_blink();
        } else {
            stop_blink();
        }
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

// ---------- Reglages et choix de la langue ----------

const SOURCE_URL: &str = "https://github.com/Splainte/Robloader";

thread_local! {
    static SETTINGS_WINDOW: RefCell<Option<Retained<NSWindow>>> = const { RefCell::new(None) };
}

/// Ouvre la fenetre Reglages (menu Robloader > Reglages… et roue du volet).
pub fn open_settings(app: &AppHandle) {
    on_main(app, show_settings_window);
}

// Francais / English cote a cote, a jour apres chaque changement de langue.
fn language_control(controller: &Controller, mtm: MainThreadMarker) -> Retained<NSSegmentedControl> {
    let labels = NSArray::from_retained_slice(&[
        NSString::from_str("Français"),
        NSString::from_str("English"),
    ]);
    let seg = unsafe {
        NSSegmentedControl::segmentedControlWithLabels_trackingMode_target_action(
            &labels,
            NSSegmentSwitchTracking::SelectOne,
            Some(controller.as_target()),
            Some(sel!(onLanguage:)),
            mtm,
        )
    };
    seg.setSelectedSegment(if i18n::en() { 1 } else { 0 });
    let c = seg.clone();
    on_relabel(move || c.setSelectedSegment(if i18n::en() { 1 } else { 0 }));
    seg
}

fn show_settings_window() {
    let Some(mtm) = MainThreadMarker::new() else { return };
    if let Some(window) = SETTINGS_WINDOW.with(|w| w.borrow().clone()) {
        window.makeKeyAndOrderFront(None);
        return;
    }
    let Some(controller) = CONTROLLER.with(|c| c.borrow().clone()) else { return };
    const WIDTH: f64 = 420.0;
    const PAD: f64 = 24.0;

    // Langue.
    let lang_label = tlabel("Langue :", "Language:", mtm);
    let seg = language_control(&controller, mtm);
    let lang_row = hstack(&[&lang_label, &seg], mtm);
    lang_row.setSpacing(10.0);
    let hint = tlabel(
        "S'applique tout de suite, sans redémarrer.",
        "Applies right away, no restart needed.",
        mtm,
    );
    hint.setFont(Some(&NSFont::systemFontOfSize(11.0)));
    hint.setTextColor(Some(&NSColor::secondaryLabelColor()));

    let separator = NSBox::new(mtm);
    separator.setBoxType(NSBoxType::Separator);

    // A propos : icone, version, licence, code source.
    let icon = NSImageView::new(mtm);
    if let Some(img) = NSApplication::sharedApplication(mtm).applicationIconImage() {
        icon.setImage(Some(&img));
    }
    icon.widthAnchor().constraintEqualToConstant(56.0).setActive(true);
    icon.heightAnchor().constraintEqualToConstant(56.0).setActive(true);
    let name = label("Robloader", mtm);
    name.setFont(Some(&NSFont::systemFontOfSize_weight(13.0, unsafe { NSFontWeightBold })));
    let version_text = {
        let v = controller.ivars().version.clone();
        move || {
            if i18n::en() {
                format!("Version {v} · MIT License")
            } else {
                format!("Version {v} · licence MIT")
            }
        }
    };
    let version = label(&version_text(), mtm);
    version.setFont(Some(&NSFont::systemFontOfSize(12.0)));
    version.setTextColor(Some(&NSColor::secondaryLabelColor()));
    {
        let l = version.clone();
        on_relabel(move || l.setStringValue(&NSString::from_str(&version_text())));
    }
    let link = tbutton(
        "Code source sur GitHub",
        "Source code on GitHub",
        &controller,
        sel!(onOpenSource:),
        mtm,
    );
    link.setBordered(false);
    link.setContentTintColor(Some(&NSColor::linkColor()));
    let about_text = vstack(&[&name, &version, &link], 3.0, mtm);
    let about = hstack(&[&icon, &about_text], mtm);
    about.setSpacing(14.0);

    let content = vstack(&[&lang_row, &hint, &separator, &about], 12.0, mtm);
    content.setEdgeInsets(NSEdgeInsets { top: 20.0, left: PAD, bottom: 22.0, right: PAD });
    content.setCustomSpacing_afterView(18.0, &hint);
    content.setCustomSpacing_afterView(18.0, &separator);
    content.setTranslatesAutoresizingMaskIntoConstraints(false);
    content.widthAnchor().constraintEqualToConstant(WIDTH).setActive(true);
    pin_width(&separator, &content, PAD);

    let window = unsafe {
        NSWindow::initWithContentRect_styleMask_backing_defer(
            NSWindow::alloc(mtm),
            NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(WIDTH, 220.0)),
            NSWindowStyleMask::Titled | NSWindowStyleMask::Closable,
            NSBackingStoreType::Buffered,
            false,
        )
    };
    unsafe { window.setReleasedWhenClosed(false) };
    window.setTitle(&NSString::from_str(tr("Réglages", "Settings")));
    {
        let w = window.clone();
        on_relabel(move || w.setTitle(&NSString::from_str(tr("Réglages", "Settings"))));
    }
    window.setContentView(Some(&content));
    window.setContentSize(content.fittingSize());
    window.center();
    window.makeKeyAndOrderFront(None);
    SETTINGS_WINDOW.with(|w| *w.borrow_mut() = Some(window));
}

/// 1er lancement sans langue enregistree (macOS n'a pas d'installeur) :
/// feuille sur la fenetre, langue du systeme preselectionnee. Le choix
/// s'applique des le clic ; « Continuer » l'enregistre.
pub fn first_run(app: &AppHandle) {
    on_main(app, show_first_run);
}

fn show_first_run() {
    let Some(mtm) = MainThreadMarker::new() else { return };
    let Some(controller) = CONTROLLER.with(|c| c.borrow().clone()) else { return };
    let Some(window) = with_ui(|ui| ui.window.clone()) else { return };

    let title = || tr("Bienvenue dans Robloader", "Welcome to Robloader");
    let body = || {
        tr(
            "Choisis la langue de l'app. Tu pourras la changer plus tard dans Réglages (⌘,).",
            "Choose the app language. You can change it later in Settings (⌘,).",
        )
    };
    let alert = NSAlert::new(mtm);
    alert.setMessageText(&NSString::from_str(title()));
    alert.setInformativeText(&NSString::from_str(body()));
    let button = alert.addButtonWithTitle(&NSString::from_str(tr("Continuer", "Continue")));
    let seg = language_control(&controller, mtm);
    seg.sizeToFit();
    alert.setAccessoryView(Some(&seg));
    {
        let (alert, button) = (alert.clone(), button.clone());
        on_relabel(move || {
            alert.setMessageText(&NSString::from_str(title()));
            alert.setInformativeText(&NSString::from_str(body()));
            button.setTitle(&NSString::from_str(tr("Continuer", "Continue")));
            alert.layout();
        });
    }
    let app = controller.ivars().app.clone();
    let done = RcBlock::new(move |_response: isize| {
        i18n::apply(&app, i18n::lang());
    });
    alert.beginSheetModalForWindow_completionHandler(&window, Some(&done));
}
