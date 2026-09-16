// ============================================================
// Interface native macOS (redesign D3, cf. docs/redesign-macos).
//
// - NSToolbar : champ de lien, pastille de source, Coller, Mise a jour,
//   Telecharger (style prominent sur macOS 26+).
// - NSSplitViewController : volet lateral systeme (Liquid Glass, suit le
//   reglage d'opacite de macOS 27) avec des controles AppKit natifs
//   (NSSegmentedControl, NSSwitch, NSPopUpButton...). La webview Tauri
//   devient le panneau de contenu et n'affiche plus que la file.
//
// Les reglages vivent ici (SETTINGS). Un clic sur Telecharger (ou Entree)
// emet "mac://download" avec le lien + les reglages ; le front cree la
// tache et appelle start_download comme avant.
// Tout ce qui touche AppKit tourne sur le thread principal.
// ============================================================

use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject, Sel};
use objc2::{define_class, msg_send, sel, DefinedClass, MainThreadOnly};
use objc2_app_kit::{
    NSAnimationContext, NSApplication, NSButton, NSColor, NSControlStateValueOff,
    NSControlStateValueOn, NSControlTextEditingDelegate, NSEventType, NSFont, NSFontWeightSemibold,
    NSImage, NSLayoutAttribute, NSLayoutConstraint, NSLineBreakMode, NSPasteboard,
    NSPasteboardTypeString, NSPopUpButton, NSScrollView, NSSearchField, NSSearchFieldDelegate,
    NSSegmentDistribution, NSSegmentSwitchTracking, NSSegmentedControl, NSSplitViewController,
    NSSplitViewItem, NSSplitViewItemBehavior, NSStackView, NSStackViewDistribution, NSSwitch,
    NSTextField, NSTextFieldDelegate, NSToolbar, NSToolbarDelegate, NSToolbarItem,
    NSToolbarItemStyle, NSToolbarSidebarTrackingSeparatorItemIdentifier,
    NSUserInterfaceLayoutOrientation, NSView, NSViewController, NSWindow, NSWindowTitleVisibility,
    NSWindowToolbarStyle,
};
use objc2_foundation::{
    ns_string, MainThreadMarker, NSArray, NSEdgeInsets, NSNotification, NSObject,
    NSObjectProtocol, NSPoint, NSRect, NSSize, NSString,
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

// ---------- Vues gardees pour les mises a jour (thread principal) ----------

struct Ui {
    window: Retained<NSWindow>,
    url_field: Retained<NSSearchField>,
    chip_item: Option<Retained<NSToolbarItem>>,
    update_item: Option<Retained<NSToolbarItem>>,
    sidebar_stack: Retained<NSStackView>,
    quality_section: Retained<NSStackView>,
    clip_switch: Retained<NSSwitch>,
    times_row: Retained<NSStackView>,
    start_field: Retained<NSTextField>,
    end_field: Retained<NSTextField>,
    transcode_switch: Retained<NSSwitch>,
    output_row: Retained<NSStackView>,
    output_popup: Retained<NSPopUpButton>,
    subs_row: Retained<NSStackView>,
    subs_switch: Retained<NSSwitch>,
    thumb_row: Retained<NSStackView>,
    thumb_switch: Retained<NSSwitch>,
    dest_label: Retained<NSTextField>,
    foot_label: Retained<NSTextField>,
    repair_button: Retained<NSButton>,
    profile_id: &'static str,
    update_version: Option<String>,
    update_installing: bool,
}

thread_local! {
    static UI: RefCell<Option<Ui>> = const { RefCell::new(None) };
    static CONTROLLER: RefCell<Option<Retained<Controller>>> = const { RefCell::new(None) };
}

fn with_ui<T>(f: impl FnOnce(&mut Ui) -> T) -> Option<T> {
    UI.with(|cell| cell.borrow_mut().as_mut().map(f))
}

// ---------- Controleur Obj-C (delegue + cible des actions) ----------

pub struct Ivars {
    app: AppHandle,
}

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
    }

    unsafe impl NSTextFieldDelegate for Controller {}
    unsafe impl NSSearchFieldDelegate for Controller {}

    impl Controller {
        #[unsafe(method(onDownload:))]
        fn on_download(&self, _sender: Option<&AnyObject>) {
            self.request_download();
        }

        // Action du champ de lien / des champs Debut-Fin : seulement sur Entree
        // (le champ de recherche l'envoie aussi quand on clique sur sa croix).
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
            with_ui(|ui| {
                ui.url_field.setStringValue(&NSString::from_str(&text));
                ui.window.makeFirstResponder(Some(&ui.url_field));
            });
            apply_profile(detect_profile(&text), true);
        }

        #[unsafe(method(onQuality:))]
        fn on_quality(&self, sender: Option<&AnyObject>) {
            let Some(control) = sender.and_then(|s| s.downcast_ref::<NSSegmentedControl>()) else {
                return;
            };
            let idx = control.selectedSegment().clamp(0, 4) as usize;
            with_settings(|s| s.quality_label = QUALITY_LABELS[idx].into());
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
                    with_ui(|ui| reveal(ui, &ui.times_row, on, true));
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
                        reveal(ui, &ui.output_row, on, true);
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
    fn new(app: AppHandle, mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(Ivars { app });
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
            ID_URL => {
                let field = UI.with(|c| c.borrow().as_ref().map(|ui| ui.url_field.clone()))?;
                item.setLabel(ns_string!("Lien"));
                item.setView(Some(&field));
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
            Which::Url(u) => apply_profile(detect_profile(&u), true),
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
        with_ui(|ui| {
            if url.is_empty() {
                ui.window.makeFirstResponder(Some(&ui.url_field));
                return;
            }
            ui.url_field.setStringValue(ns_string!(""));
            ui.start_field.setStringValue(ns_string!(""));
            ui.end_field.setStringValue(ns_string!(""));
        });
        if !url.is_empty() {
            with_settings(|s| {
                s.start.clear();
                s.end.clear();
            });
            apply_profile(&DEFAULT_PROFILE, true);
        }
    }
}

fn toolbar_identifiers() -> Retained<NSArray<NSString>> {
    let sep: &NSString = unsafe { NSToolbarSidebarTrackingSeparatorItemIdentifier };
    let ids = [
        NSString::from_str(&sep.to_string()),
        NSString::from_str(ID_URL),
        NSString::from_str(ID_CHIP),
        NSString::from_str(ID_PASTE),
        NSString::from_str(ID_UPDATE),
        NSString::from_str(ID_DOWNLOAD),
    ];
    NSArray::from_retained_slice(&ids)
}

// ---------- Aides ----------

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

// Montre/masque une ligne du volet avec l'animation de pile d'AppKit.
fn reveal(ui: &Ui, view: &NSView, show: bool, animate: bool) {
    if view.isHidden() != show {
        return;
    }
    if !animate {
        view.setHidden(!show);
        view.setAlphaValue(if show { 1.0 } else { 0.0 });
        return;
    }
    NSAnimationContext::beginGrouping();
    let ctx = NSAnimationContext::currentContext();
    ctx.setDuration(0.3);
    ctx.setAllowsImplicitAnimation(true);
    view.setHidden(!show);
    view.setAlphaValue(if show { 1.0 } else { 0.0 });
    ui.sidebar_stack.layoutSubtreeIfNeeded();
    NSAnimationContext::endGrouping();
}

fn fill_outputs(popup: &NSPopUpButton, transcode: bool, selected: &str) {
    let list: &[&str] = if transcode { &OUTPUTS_TRANSCODE } else { &OUTPUTS_NATIVE };
    popup.removeAllItems();
    let titles: Vec<Retained<NSString>> = list.iter().map(|t| NSString::from_str(t)).collect();
    popup.addItemsWithTitles(&NSArray::from_retained_slice(&titles));
    popup.selectItemWithTitle(&NSString::from_str(selected));
}

// Adapte le champ, la pastille et les sections au site detecte.
fn apply_profile(profile: &'static Profile, animate: bool) {
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
        reveal(ui, &ui.quality_section, profile.ladder, animate);
        reveal(ui, &ui.subs_row, profile.subtitles, animate);
        reveal(ui, &ui.thumb_row, profile.thumbnail, animate);
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

// Ligne « libelle ........ controle », le libelle prend la place libre.
fn row(text: &str, control: &NSView, mtm: MainThreadMarker) -> Retained<NSStackView> {
    let l = label(text, mtm);
    l.setLineBreakMode(NSLineBreakMode::ByTruncatingTail);
    l.setContentHuggingPriority_forOrientation(1.0, NSLayoutConstraintOrientation::Horizontal);
    l.setContentCompressionResistancePriority_forOrientation(
        250.0,
        NSLayoutConstraintOrientation::Horizontal,
    );
    let r = hstack(&[&l, control], mtm);
    r.setDistribution(NSStackViewDistribution::Fill);
    r
}

use objc2_app_kit::NSLayoutConstraintOrientation;

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

fn build_sidebar(
    controller: &Controller,
    window: Retained<NSWindow>,
    mtm: MainThreadMarker,
) -> (Retained<NSView>, Ui) {
    let settings = with_settings(|s| s.clone());

    // Qualite : controle segmente natif (un seul choix).
    let labels: Vec<Retained<NSString>> = QUALITY_SHORT.iter().map(|q| NSString::from_str(q)).collect();
    let quality = unsafe {
        NSSegmentedControl::segmentedControlWithLabels_trackingMode_target_action(
            &NSArray::from_retained_slice(&labels),
            NSSegmentSwitchTracking::SelectOne,
            Some(controller.as_target()),
            Some(sel!(onQuality:)),
            mtm,
        )
    };
    quality.setSegmentDistribution(NSSegmentDistribution::FillEqually);
    quality.setSelectedSegment(0);
    let quality_header = section_header("Qualité", mtm);
    let quality_section = vstack(&[&quality_header, &quality], 8.0, mtm);

    // Options.
    let options_header = section_header("Options", mtm);
    let clip_switch = new_switch(controller, settings.clip, mtm);
    let clip_row = row("Extraire un passage", &clip_switch, mtm);

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

    let transcode_switch = new_switch(controller, settings.transcode, mtm);
    let transcode_row = row("Transcodage", &transcode_switch, mtm);

    let output_popup = NSPopUpButton::initWithFrame_pullsDown(
        NSPopUpButton::alloc(mtm),
        NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(140.0, 24.0)),
        false,
    );
    fill_outputs(&output_popup, settings.transcode, &settings.output);
    unsafe {
        output_popup.setTarget(Some(controller.as_target()));
        output_popup.setAction(Some(sel!(onOutput:)));
    }
    let output_row = row("Format de sortie", &output_popup, mtm);

    let subs_switch = new_switch(controller, settings.subs, mtm);
    let subs_row = row("Sous-titres (.srt)", &subs_switch, mtm);
    let thumb_switch = new_switch(controller, settings.thumb, mtm);
    let thumb_row = row("Miniature", &thumb_switch, mtm);

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
            &quality_section,
            &options_header,
            &clip_row,
            &times_row,
            &transcode_row,
            &output_row,
            &subs_row,
            &thumb_row,
            &dest_header,
            &dest_label,
            &dest_buttons,
            &foot_label,
            &repair_button,
        ],
        10.0,
        mtm,
    );
    stack.setEdgeInsets(NSEdgeInsets { top: 10.0, left: 16.0, bottom: 16.0, right: 16.0 });
    stack.setCustomSpacing_afterView(22.0, &quality_section);
    stack.setCustomSpacing_afterView(22.0, &thumb_row);
    stack.setCustomSpacing_afterView(22.0, &dest_buttons);
    stack.setTranslatesAutoresizingMaskIntoConstraints(false);

    // Lignes pleine largeur.
    for v in [
        &*quality_section as &NSView,
        &quality,
        &clip_row,
        &times_row,
        &transcode_row,
        &output_row,
        &subs_row,
        &thumb_row,
        &dest_label,
        &dest_buttons,
        &foot_label,
    ] {
        if v as *const NSView == &*quality as *const NSSegmentedControl as *const NSView {
            pin_width(v, &quality_section, 0.0);
        } else {
            pin_width(v, &stack, 16.0);
        }
    }
    for r in [&clip_row, &transcode_row, &output_row, &subs_row, &thumb_row] {
        r.heightAnchor().constraintGreaterThanOrEqualToConstant(26.0).setActive(true);
    }

    // Etat initial des revelations.
    times_row.setHidden(!settings.clip);
    times_row.setAlphaValue(if settings.clip { 1.0 } else { 0.0 });
    output_row.setHidden(!settings.transcode);
    output_row.setAlphaValue(if settings.transcode { 1.0 } else { 0.0 });

    // Defilement si la fenetre est basse (les marges sous la barre d'outils
    // sont gerees par automaticallyAdjustsContentInsets).
    let document: Retained<FlippedView> = unsafe { msg_send![FlippedView::alloc(mtm), init] };
    document.setTranslatesAutoresizingMaskIntoConstraints(false);
    document.addSubview(&stack);
    let scroll = NSScrollView::new(mtm);
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

    // Champ de lien (pose dans la barre d'outils par le delegue).
    let url_field = NSSearchField::new(mtm);
    url_field.setPlaceholderString(Some(&NSString::from_str(DEFAULT_PROFILE.placeholder)));
    unsafe {
        url_field.setDelegate(Some(ProtocolObject::from_ref(controller)));
        url_field.setTarget(Some(controller.as_target()));
        url_field.setAction(Some(sel!(onSubmit:)));
        let _: () = msg_send![&*url_field, setSendsWholeSearchString: true];
        // Icone de lien a la place de la loupe.
        let cell: *mut AnyObject = msg_send![&*url_field, cell];
        if !cell.is_null() {
            let button_cell: *mut AnyObject = msg_send![cell, searchButtonCell];
            if let (false, Some(img)) = (button_cell.is_null(), symbol("link", "Lien")) {
                let _: () = msg_send![button_cell, setImage: &*img];
            }
        }
    }
    url_field.widthAnchor().constraintGreaterThanOrEqualToConstant(220.0).setActive(true);
    let max = url_field.widthAnchor().constraintLessThanOrEqualToConstant(900.0);
    max.setActive(true);

    let ui = Ui {
        window,
        url_field,
        chip_item: None,
        update_item: None,
        sidebar_stack: stack,
        quality_section,
        clip_switch,
        times_row,
        start_field,
        end_field,
        transcode_switch,
        output_row,
        output_popup,
        subs_row,
        subs_switch,
        thumb_row,
        thumb_switch,
        dest_label,
        foot_label,
        repair_button,
        profile_id: DEFAULT_PROFILE.id,
        update_version: None,
        update_installing: false,
    };
    (Retained::into_super(scroll), ui)
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

    let controller = Controller::new(app.clone(), mtm);
    let (sidebar_view, ui) = build_sidebar(&controller, ns_window.clone(), mtm);

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

    // Barre d'outils unifiee.
    let toolbar = NSToolbar::initWithIdentifier(NSToolbar::alloc(mtm), ns_string!("RobloaderToolbar"));
    toolbar.setDelegate(Some(ProtocolObject::from_ref(&*controller)));
    toolbar.setAllowsUserCustomization(false);
    toolbar.setDisplayMode(objc2_app_kit::NSToolbarDisplayMode::IconOnly);
    ns_window.setToolbar(Some(&toolbar));
    ns_window.setToolbarStyle(NSWindowToolbarStyle::Unified);
    ns_window.setTitleVisibility(NSWindowTitleVisibility::Visible);
    ns_window.setTitle(&NSString::from_str(&format!(
        "Robloader {}",
        app.package_info().version
    )));
    INSTALLED.store(true, Ordering::Relaxed);
    true
}

// ---------- Commandes appelees par le front ----------

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
            ui.update_version = version.clone();
            ui.update_installing = installing;
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
