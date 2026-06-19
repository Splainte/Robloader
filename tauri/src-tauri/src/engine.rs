// ============================================================
//  Moteur Robloader (port Rust de la logique de Robloader.py)
//  On orchestre les binaires yt-dlp + ffmpeg/ffprobe (comme le
//  faisait le Python via subprocess) et on pousse l'avancement
//  vers l'UI Tauri par evenements `task://update`.
// ============================================================

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

// ---------- Constantes reprises de Robloader.py ----------

const COOKIE_FIX_URL: &str =
    "https://docs.google.com/document/d/1zCuLswlQeOCV-C7bQWlmi6Ix-OcmPy252RCZSjAeKF4/";

// yt-dlp : clients player YouTube a essayer (cf. YOUTUBE_EXTRACTOR_ARGS).
const YOUTUBE_EXTRACTOR_ARGS: &str = "youtube:player_client=default,android_vr,web_embedded";
// Tri des formats : audio ORIGINAL d'abord (lang), puis res/fps, on prefere H.264/AAC (cf. FORMAT_SORT).
const FORMAT_SORT: &str = "lang,res,fps,vcodec:h264,acodec:aac,br";
// Resolution du nsig (4K/1440p) via Deno + solveur EJS distant.
const REMOTE_COMPONENTS: &str = "ejs:github";
// Codecs deja prets pour Premiere Pro (pas de transcodage necessaire).
const PREMIERE_READY_CODECS: &[&str] = &["h264", "avc1", "avc", "hevc", "h265", "hev1", "hvc1"];

// Marqueurs d'erreur "cookies/connexion requise" -> message clair + bouton "Reparer".
const COOKIE_ERR_MARKERS: &[&str] = &[
    "sign in to confirm",
    "not a bot",
    "confirm your age",
    "age-restricted",
    "age restricted",
    "only available to members",
    "members-only",
    "join this channel",
    "cookies",
    "cookie",
    "use --cookies",
    "http error 403",
    "403: forbidden",
];

// ---------- Etat partage ----------

#[derive(Clone)]
pub struct TaskHandle {
    cancel: Arc<AtomicBool>,
    child: Arc<Mutex<Option<std::process::Child>>>,
}

impl TaskHandle {
    fn new() -> Self {
        TaskHandle {
            cancel: Arc::new(AtomicBool::new(false)),
            child: Arc::new(Mutex::new(None)),
        }
    }
    fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }
}

#[derive(Default)]
pub struct Engine {
    tasks: Arc<Mutex<HashMap<u64, TaskHandle>>>,
}

// ---------- Messages UI ----------

#[derive(Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
struct TaskUpdate {
    id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    status_kind: Option<String>, // "info" | "warn" | "ok" | "err"
    #[serde(skip_serializing_if = "Option::is_none")]
    percent: Option<f64>, // 0.0..1.0
    #[serde(skip_serializing_if = "Option::is_none")]
    indeterminate: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    action: Option<String>, // "cancel" | "open" | "repair" | "none"
    #[serde(skip_serializing_if = "Option::is_none")]
    final_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    done: Option<bool>,
}

fn emit(app: &AppHandle, u: TaskUpdate) {
    let _ = app.emit("task://update", u);
}

// Raccourci : maj de statut (texte + couleur logique).
fn status(app: &AppHandle, id: u64, text: &str, kind: &str) {
    emit(
        app,
        TaskUpdate {
            id,
            status: Some(text.to_string()),
            status_kind: Some(kind.to_string()),
            ..Default::default()
        },
    );
}

// ---------- Options de telechargement (depuis l'UI) ----------

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DownloadOpts {
    id: u64,
    url: String,
    #[serde(default)]
    start: String,
    #[serde(default)]
    end: String,
    quality_label: String,
    output: String,
    subs: bool,
    thumb: bool,
    transcode: bool,
    #[serde(default)]
    download_dir: Option<String>,
}

// ---------- Sorties possibles ----------

const OUT_HEVC: &str = "HEVC";
const OUT_PRORES: &str = "ProRes";
const OUT_VIDEO_NATIVE: &str = "Vidéo (natif)";
const OUT_MP3: &str = "Audio MP3";
const OUT_WAV: &str = "Audio WAV";
const OUT_SUBS: &str = "Sous-titres seuls (.srt)";

// ---------- Helpers chemins / systeme ----------

fn home_dir() -> PathBuf {
    if cfg!(windows) {
        std::env::var("USERPROFILE")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("."))
    } else {
        std::env::var("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("."))
    }
}

pub fn config_dir() -> PathBuf {
    let home = home_dir();
    if cfg!(windows) {
        std::env::var("APPDATA")
            .map(PathBuf::from)
            .unwrap_or(home)
            .join("Robloader")
    } else if cfg!(target_os = "macos") {
        home.join("Library").join("Application Support").join("Robloader")
    } else {
        std::env::var("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| home.join(".config"))
            .join("Robloader")
    }
}

fn default_downloads_dir() -> PathBuf {
    let home = home_dir();
    // Linux : respecter xdg-user-dirs (dossier relocalisable).
    if !cfg!(windows) && !cfg!(target_os = "macos") {
        if let Ok(out) = Command::new("xdg-user-dir").arg("DOWNLOAD").output() {
            if out.status.success() {
                let d = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !d.is_empty() && Path::new(&d).is_dir() {
                    return PathBuf::from(d);
                }
            }
        }
    }
    home.join("Downloads")
}

// Resolution d'un binaire : bundle (resources / a cote de l'exe) puis PATH.
fn resolve_binary(app: &AppHandle, name: &str) -> String {
    let exe_name = if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    };
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Ok(res) = app.path().resource_dir() {
        dirs.push(res.clone());
        dirs.push(res.join("bin"));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(p) = exe.parent() {
            dirs.push(p.to_path_buf());
            dirs.push(p.join("bin"));
        }
    }
    for d in dirs {
        let cand = d.join(&exe_name);
        if cand.exists() {
            return cand.to_string_lossy().to_string();
        }
    }
    // Sinon on laisse le PATH resoudre le nom court.
    name.to_string()
}

// Deno present ? (requis pour le nsig YouTube -> 4K).
fn has_js_runtime() -> bool {
    which("deno")
}

fn which(name: &str) -> bool {
    let path = std::env::var_os("PATH").unwrap_or_default();
    let exe = if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    };
    std::env::split_paths(&path).any(|p| p.join(&exe).exists())
}

// Liste ordonnee des navigateurs presents (cookies-from-browser), Firefox en premier.
fn detect_browsers() -> Vec<String> {
    let home = home_dir();
    let cand: Vec<(&str, PathBuf)> = if cfg!(target_os = "macos") {
        vec![
            ("firefox", home.join("Library/Application Support/Firefox/Profiles")),
            ("chrome", home.join("Library/Application Support/Google/Chrome/Local State")),
            ("brave", home.join("Library/Application Support/BraveSoftware/Brave-Browser/Local State")),
            ("edge", home.join("Library/Application Support/Microsoft Edge/Local State")),
        ]
    } else if cfg!(windows) {
        let la = std::env::var("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(|_| home.clone());
        let ap = std::env::var("APPDATA").map(PathBuf::from).unwrap_or_else(|_| home.clone());
        vec![
            ("firefox", ap.join("Mozilla/Firefox/Profiles")),
            ("chrome", la.join("Google/Chrome/User Data/Local State")),
            ("edge", la.join("Microsoft/Edge/User Data/Local State")),
            ("brave", la.join("BraveSoftware/Brave-Browser/User Data/Local State")),
        ]
    } else {
        vec![
            ("firefox", home.join(".mozilla/firefox")),
            ("chrome", home.join(".config/google-chrome/Local State")),
        ]
    };
    cand.into_iter()
        .filter(|(_, p)| p.exists())
        .map(|(n, _)| n.to_string())
        .collect()
}

fn cookie_path(app: &AppHandle) -> Option<PathBuf> {
    // cookies.txt fourni a cote de l'app / dans les resources / dans le home.
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Ok(res) = app.path().resource_dir() {
        dirs.push(res);
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(p) = exe.parent() {
            dirs.push(p.to_path_buf());
        }
    }
    dirs.push(home_dir());
    dirs.into_iter().map(|d| d.join("cookies.txt")).find(|p| p.exists())
}

// ---------- Qualite / format ----------

fn quality_height(label: &str) -> Option<u32> {
    if label.starts_with("1440") {
        Some(1440)
    } else if label.starts_with("1080") {
        Some(1080)
    } else if label.starts_with("720") {
        Some(720)
    } else if label.starts_with("480") {
        Some(480)
    } else {
        None // "Qualité max"
    }
}

fn format_for_height(h: Option<u32>) -> String {
    match h {
        None => "bv*+ba/b".to_string(),
        Some(h) => format!("bv*[height<={h}]+ba/bv*[height<={h}]/b[height<={h}]/b"),
    }
}

fn is_youtube(url: &str) -> bool {
    let u = url.trim().to_lowercase();
    let host = u
        .split("://")
        .last()
        .unwrap_or("")
        .split('/')
        .next()
        .unwrap_or("");
    host == "youtube.com"
        || host.ends_with(".youtube.com")
        || host == "youtu.be"
        || host.ends_with(".youtu.be")
}

// Profil simplifie : seul YouTube a un quality_ladder (cf. SITE_PROFILES). Les autres -> meilleur flux.
fn has_quality_ladder(url: &str) -> bool {
    is_youtube(url)
}

// ---------- Timecode ----------

fn parse_timecode(tc: &str) -> Option<f64> {
    let s = tc.trim();
    if s.is_empty() {
        return None;
    }
    let parts: Vec<&str> = s.split(':').collect();
    match parts.len() {
        1 => parts[0].parse::<f64>().ok(),
        2 => {
            let m = parts[0].parse::<f64>().ok()?;
            let sec = parts[1].parse::<f64>().ok()?;
            Some(m * 60.0 + sec)
        }
        3 => {
            let h = parts[0].parse::<f64>().ok()?;
            let m = parts[1].parse::<f64>().ok()?;
            let sec = parts[2].parse::<f64>().ok()?;
            Some(h * 3600.0 + m * 60.0 + sec)
        }
        _ => None,
    }
}

// ---------- Nettoyage du nom de fichier ----------

fn sanitize_title(s: &str) -> String {
    let cleaned: String = s
        .chars()
        .filter(|c| c.is_alphanumeric() || " .-_()".contains(*c))
        .collect();
    cleaned.trim_end().to_string()
}

// ---------- Cookies : sources a essayer dans l'ordre ----------

#[derive(Clone)]
enum CookieAttempt {
    File(String),
    Browser(String),
    None,
}

fn cookie_attempts(app: &AppHandle) -> Vec<CookieAttempt> {
    let mut v = Vec::new();
    if let Some(p) = cookie_path(app) {
        v.push(CookieAttempt::File(p.to_string_lossy().to_string()));
    }
    for b in detect_browsers() {
        v.push(CookieAttempt::Browser(b));
    }
    v.push(CookieAttempt::None);
    v
}

fn apply_cookies(cmd: &mut Command, attempt: &CookieAttempt) {
    match attempt {
        CookieAttempt::File(p) => {
            cmd.arg("--cookies").arg(p);
        }
        CookieAttempt::Browser(b) => {
            cmd.arg("--cookies-from-browser").arg(b);
        }
        CookieAttempt::None => {}
    }
}

// ---------- Lancement de process avec annulation + lecture stdout ----------

#[cfg(windows)]
fn no_window(cmd: &mut Command) {
    use std::os::windows::process::CommandExt;
    cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
}
#[cfg(not(windows))]
fn no_window(_cmd: &mut Command) {}

// Lance `cmd`, draine stderr (pour les erreurs), appelle `on_line` pour chaque
// ligne de stdout. Retourne (code de sortie, stderr accumule).
fn run_proc<F: FnMut(&str)>(
    handle: &TaskHandle,
    mut cmd: Command,
    mut on_line: F,
) -> std::io::Result<(i32, String)> {
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).stdin(Stdio::null());
    no_window(&mut cmd);

    let mut child = cmd.spawn()?;
    let stdout = child.stdout.take().expect("stdout");
    let stderr = child.stderr.take().expect("stderr");

    // On stocke l'enfant pour qu'`cancel_download` puisse le tuer.
    *handle.child.lock().unwrap() = Some(child);
    if handle.cancelled() {
        if let Some(c) = handle.child.lock().unwrap().as_mut() {
            let _ = c.kill();
        }
    }

    // stderr draine dans un thread (evite tout blocage de pipe) + garde la fin pour les erreurs.
    let err_buf = Arc::new(Mutex::new(String::new()));
    let eb = err_buf.clone();
    let err_thread = std::thread::spawn(move || {
        let mut raw = stderr;
        let mut chunk = String::new();
        // lecture brute (les lignes ffmpeg/yt-dlp utilisent \r) -> on lit tout puis on garde la fin.
        let mut buf = [0u8; 4096];
        loop {
            match raw.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => chunk.push_str(&String::from_utf8_lossy(&buf[..n])),
                Err(_) => break,
            }
        }
        let mut g = eb.lock().unwrap();
        g.push_str(&chunk);
    });

    // stdout ligne a ligne (progress).
    let reader = BufReader::new(stdout);
    for line in reader.lines().map_while(Result::ok) {
        if handle.cancelled() {
            break;
        }
        on_line(&line);
    }

    // Attente + recuperation du code.
    let code = {
        let mut taken = handle.child.lock().unwrap().take();
        match taken.as_mut() {
            Some(c) => c.wait().ok().and_then(|s| s.code()).unwrap_or(-1),
            None => -1,
        }
    };
    let _ = err_thread.join();
    let errs = err_buf.lock().unwrap().clone();
    Ok((code, errs))
}

// yt-dlp : telechargement avec barre de progression (parse RLPROG sur stdout).
fn run_ytdlp_download(
    handle: &TaskHandle,
    app: &AppHandle,
    id: u64,
    cmd: Command,
    status_text: &str,
) -> std::io::Result<(i32, String)> {
    let st = status_text.to_string();
    run_proc(handle, cmd, |line| {
        if let Some(rest) = line.strip_prefix("RLPROG ") {
            let p: Vec<&str> = rest.split_whitespace().collect();
            if p.len() == 3 {
                let dl = p[0].parse::<f64>().ok();
                let total = p[1]
                    .parse::<f64>()
                    .ok()
                    .or_else(|| p[2].parse::<f64>().ok());
                if let (Some(dl), Some(total)) = (dl, total) {
                    if total > 0.0 {
                        let pct = (dl / total).clamp(0.0, 1.0);
                        emit(
                            app,
                            TaskUpdate {
                                id,
                                status: Some(format!("{} {}%", st, (pct * 100.0) as i32)),
                                status_kind: Some("info".into()),
                                percent: Some(pct),
                                indeterminate: Some(false),
                                ..Default::default()
                            },
                        );
                    }
                }
            }
        }
    })
}

// ffmpeg : conversion avec progression (parse out_time_us sur stdout).
fn run_ffmpeg(
    handle: &TaskHandle,
    app: &AppHandle,
    id: u64,
    ffmpeg: &str,
    args: &[String],
    duration: f64,
    step_text: &str,
) -> std::io::Result<i32> {
    let mut cmd = Command::new(ffmpeg);
    cmd.args(args);
    let step = step_text.to_string();
    let mut last = -1i32;
    let (code, _err) = run_proc(handle, cmd, |line| {
        if let Some(v) = line.strip_prefix("out_time_us=") {
            if let Ok(us) = v.trim().parse::<i64>() {
                if duration > 0.0 {
                    let pct = ((us as f64) / (duration * 1_000_000.0)).clamp(0.0, 1.0);
                    let p100 = (pct * 100.0) as i32;
                    if p100 != last {
                        last = p100;
                        emit(
                            app,
                            TaskUpdate {
                                id,
                                status: Some(format!("{} {}%", step, p100)),
                                status_kind: Some("info".into()),
                                percent: Some(pct),
                                indeterminate: Some(false),
                                ..Default::default()
                            },
                        );
                    }
                }
            }
        }
    })?;
    Ok(code)
}

// ---------- Sondage codec video ----------

fn probe_video_codec(ffprobe: &str, path: &Path) -> String {
    let mut cmd = Command::new(ffprobe);
    cmd.args([
        "-v",
        "error",
        "-select_streams",
        "v:0",
        "-show_entries",
        "stream=codec_name",
        "-of",
        "default=nw=1:nk=1",
    ])
    .arg(path);
    no_window(&mut cmd);
    match cmd.output() {
        Ok(o) => String::from_utf8_lossy(&o.stdout).trim().to_lowercase(),
        Err(_) => String::new(),
    }
}

// ---------- Utilitaires fichiers ----------

fn files_with_prefix(dir: &Path, prefix: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            if let Some(name) = e.file_name().to_str() {
                if name.starts_with(prefix) {
                    out.push(e.path());
                }
            }
        }
    }
    out.sort();
    out
}

fn ext_lower(p: &Path) -> String {
    p.extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_lowercase())
        .unwrap_or_default()
}

fn is_cookie_error(msg: &str) -> bool {
    let low = msg.to_lowercase();
    COOKIE_ERR_MARKERS.iter().any(|m| low.contains(m))
}

// ============================================================
//  Pipeline principal (port de download_pipeline)
// ============================================================

struct Bins {
    ytdlp: String,
    ffmpeg: String,
    ffprobe: String,
}

fn download_pipeline(app: &AppHandle, handle: &TaskHandle, opts: &DownloadOpts, bins: &Bins) {
    let id = opts.id;
    let url = opts.url.trim().to_string();

    let download_dir = opts
        .download_dir
        .clone()
        .filter(|d| !d.is_empty() && Path::new(d).is_dir())
        .map(PathBuf::from)
        .unwrap_or_else(default_downloads_dir);
    let _ = std::fs::create_dir_all(&download_dir);

    match run_pipeline_inner(app, handle, opts, bins, &url, &download_dir) {
        Ok(_) => {}
        Err(e) => {
            if handle.cancelled() {
                status(app, id, "Annulé.", "warn");
                emit(
                    app,
                    TaskUpdate {
                        id,
                        action: Some("none".into()),
                        percent: Some(0.0),
                        indeterminate: Some(false),
                        done: Some(true),
                        ..Default::default()
                    },
                );
            } else if is_cookie_error(&e) {
                status(
                    app,
                    id,
                    "YouTube demande une connexion (vérification anti-robot). \
                     Tes cookies sont absents ou expirés — clique sur « Réparer ».",
                    "err",
                );
                emit(
                    app,
                    TaskUpdate {
                        id,
                        action: Some("repair".into()),
                        percent: Some(0.0),
                        indeterminate: Some(false),
                        done: Some(true),
                        ..Default::default()
                    },
                );
            } else {
                status(app, id, &format!("Échec : {e}"), "err");
                emit(
                    app,
                    TaskUpdate {
                        id,
                        action: Some("none".into()),
                        percent: Some(0.0),
                        indeterminate: Some(false),
                        done: Some(true),
                        ..Default::default()
                    },
                );
            }
        }
    }
}

fn check_cancel(handle: &TaskHandle) -> Result<(), String> {
    if handle.cancelled() {
        Err("Annulé par l'utilisateur".into())
    } else {
        Ok(())
    }
}

// Construit la base d'arguments yt-dlp communs (extractor, tri, ffmpeg, nsig).
fn base_ytdlp_args(cmd: &mut Command, bins: &Bins, remote: bool) {
    cmd.arg("--no-playlist")
        .arg("--no-warnings")
        .arg("--extractor-args")
        .arg(YOUTUBE_EXTRACTOR_ARGS)
        .arg("--ffmpeg-location")
        .arg(&bins.ffmpeg);
    if remote {
        cmd.arg("--remote-components").arg(REMOTE_COMPONENTS);
    }
}

fn add_subs_thumb(cmd: &mut Command, subs: bool, thumb: bool) {
    if subs {
        cmd.arg("--write-subs")
            .arg("--write-auto-subs")
            .arg("--sub-langs")
            .arg("fr,en")
            .arg("--convert-subs")
            .arg("srt");
    }
    if thumb {
        cmd.arg("--write-thumbnail")
            .arg("--convert-thumbnails")
            .arg("jpg");
    }
}

fn run_pipeline_inner(
    app: &AppHandle,
    handle: &TaskHandle,
    opts: &DownloadOpts,
    bins: &Bins,
    url: &str,
    download_dir: &Path,
) -> Result<(), String> {
    let id = opts.id;
    let start_seconds = parse_timecode(&opts.start);
    let end_seconds = parse_timecode(&opts.end);

    let yt = is_youtube(url);
    let js_runtime = has_js_runtime();

    // Hauteur cible (sans Deno -> plafond 1080p sur YouTube).
    let mut target_h = quality_height(&opts.quality_label);
    if !has_quality_ladder(url) {
        target_h = None;
    } else if !js_runtime {
        target_h = Some(target_h.unwrap_or(1080).min(1080));
    }
    let dl_format = format_for_height(target_h);
    let fb_h = match target_h {
        None => 1080,
        Some(h) => h.min(1080),
    };
    let fallback_format = format_for_height(Some(fb_h));
    let remote = js_runtime && yt;

    let out = opts.output.as_str();
    let audio_mode = out == OUT_MP3 || out == OUT_WAV;
    let want_prores = out == OUT_PRORES;
    let subs_only = out == OUT_SUBS;
    let native_video = out == OUT_VIDEO_NATIVE
        || (!opts.transcode && !audio_mode && !subs_only && !want_prores && out != OUT_HEVC);
    let audio_codec = if out == OUT_MP3 { "mp3" } else { "wav" };

    // ---- Analyse (extract info) ----
    emit(
        app,
        TaskUpdate {
            id,
            status: Some("Analyse de la vidéo…".into()),
            status_kind: Some("info".into()),
            indeterminate: Some(true),
            ..Default::default()
        },
    );

    let mut info_json: Option<serde_json::Value> = None;
    let mut used_cookie = CookieAttempt::None;
    let mut last_err = String::from("Analyse impossible");
    let attempts = cookie_attempts(app);
    for (i, attempt) in attempts.iter().enumerate() {
        check_cancel(handle)?;
        if i > 0 {
            if let CookieAttempt::None = attempt {
                status(
                    app,
                    id,
                    "Cookies indisponibles — analyse sans cookies (4K limitée)…",
                    "warn",
                );
            }
        }
        let mut cmd = Command::new(&bins.ytdlp);
        base_ytdlp_args(&mut cmd, bins, remote);
        cmd.arg("-f")
            .arg(&dl_format)
            .arg("-S")
            .arg(FORMAT_SORT)
            .arg("--dump-single-json")
            .arg("--no-download");
        apply_cookies(&mut cmd, attempt);
        cmd.arg(url);

        let mut json_buf = String::new();
        let (code, err) = run_proc(handle, cmd, |line| {
            json_buf.push_str(line);
            json_buf.push('\n');
        })
        .map_err(|e| e.to_string())?;
        if code == 0 {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(json_buf.trim()) {
                info_json = Some(v);
                used_cookie = attempt.clone();
                break;
            }
        }
        if !err.trim().is_empty() {
            last_err = err.trim().lines().last().unwrap_or("Analyse impossible").to_string();
        }
    }
    let info = info_json.ok_or(last_err)?;
    check_cancel(handle)?;

    let video_title = info
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("Vidéo")
        .to_string();
    let video_id = info
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("temp")
        .to_string();
    let total_duration = info.get("duration").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let channel = info
        .get("channel")
        .and_then(|v| v.as_str())
        .or_else(|| info.get("uploader").and_then(|v| v.as_str()))
        .unwrap_or("")
        .to_string();

    // Titre affiche / nom de fichier.
    let mut display = video_title.clone();
    if !channel.is_empty() {
        display.push_str(&format!(" - {channel}"));
    }
    if !opts.start.is_empty() || !opts.end.is_empty() {
        let s = if opts.start.is_empty() { "00:00" } else { &opts.start };
        let e = if opts.end.is_empty() { "Fin" } else { &opts.end };
        display.push_str(&format!(" (Extrait {s} - {e})"));
    }
    let mut safe_title = sanitize_title(&display);
    if safe_title.is_empty() {
        safe_title = format!("video_{video_id}");
    }
    emit(
        app,
        TaskUpdate {
            id,
            title: Some(safe_title.clone()),
            ..Default::default()
        },
    );

    // Plage temporelle.
    let has_range = start_seconds.is_some() || end_seconds.is_some();
    let (s_val, _e_val, segment_duration) = if has_range {
        let s = start_seconds.unwrap_or(0.0);
        let e = end_seconds.unwrap_or(total_duration);
        (s, e, (e - s).max(1.0))
    } else {
        (0.0, total_duration, total_duration)
    };
    let seek: Vec<String> = if has_range {
        vec!["-ss".into(), s_val.to_string()]
    } else {
        vec![]
    };
    let dur: Vec<String> = if has_range {
        vec!["-t".into(), segment_duration.to_string()]
    } else {
        vec![]
    };

    let dir_str = download_dir.to_string_lossy().to_string();
    let final_path: PathBuf;

    if subs_only {
        // ---- Sous-titres seuls ----
        status(app, id, "Téléchargement des sous-titres…", "info");
        emit(app, TaskUpdate { id, indeterminate: Some(true), ..Default::default() });
        let mut cmd = Command::new(&bins.ytdlp);
        base_ytdlp_args(&mut cmd, bins, remote);
        cmd.arg("--skip-download")
            .arg("--write-subs")
            .arg("--write-auto-subs")
            .arg("--sub-langs")
            .arg("fr,en")
            .arg("--convert-subs")
            .arg("srt")
            .arg("-o")
            .arg(download_dir.join(format!("{safe_title}.%(ext)s")));
        apply_cookies(&mut cmd, &used_cookie);
        cmd.arg(url);
        let (_code, _err) = run_proc(handle, cmd, |_| {}).map_err(|e| e.to_string())?;
        check_cancel(handle)?;
        let srts: Vec<PathBuf> = files_with_prefix(download_dir, &safe_title)
            .into_iter()
            .filter(|p| ext_lower(p) == "srt")
            .collect();
        final_path = srts
            .into_iter()
            .next()
            .ok_or("Aucun sous-titre disponible pour cette vidéo.")?;
    } else if audio_mode {
        // ---- Audio seul : DL complet natif puis decoupe/conversion ----
        status(app, id, "Téléchargement audio…", "info");
        let temp_tmpl = download_dir.join(format!("temp_{video_id}.%(ext)s"));
        let mut cmd = Command::new(&bins.ytdlp);
        base_ytdlp_args(&mut cmd, bins, remote);
        cmd.arg("-f")
            .arg("bestaudio/best")
            .arg("-o")
            .arg(&temp_tmpl)
            .arg("--newline")
            .arg("--progress-template")
            .arg("download:RLPROG %(progress.downloaded_bytes)s %(progress.total_bytes)s %(progress.total_bytes_estimate)s");
        add_subs_thumb(&mut cmd, opts.subs, opts.thumb);
        apply_cookies(&mut cmd, &used_cookie);
        cmd.arg(url);
        let (code, err) = run_ytdlp_download(handle, app, id, cmd, "Téléchargement audio…")
            .map_err(|e| e.to_string())?;
        check_cancel(handle)?;
        if code != 0 {
            return Err(if err.trim().is_empty() {
                "Téléchargement audio échoué.".into()
            } else {
                err.trim().lines().last().unwrap_or("Téléchargement audio échoué.").to_string()
            });
        }
        let src = files_with_prefix(download_dir, &format!("temp_{video_id}"))
            .into_iter()
            .find(|p| {
                let e = ext_lower(p);
                !["srt", "jpg", "jpeg", "png", "webp", "part"].contains(&e.as_str())
            })
            .ok_or("Téléchargement audio échoué.")?;

        status(app, id, "Conversion audio…", "info");
        final_path = download_dir.join(format!("{safe_title}.{audio_codec}"));
        let mut args: Vec<String> = vec!["-y".into(), "-progress".into(), "pipe:1".into()];
        args.extend(seek.clone());
        args.push("-i".into());
        args.push(src.to_string_lossy().to_string());
        args.extend(dur.clone());
        args.push("-vn".into());
        if audio_codec == "mp3" {
            args.extend(["-c:a", "libmp3lame", "-q:a", "2"].map(String::from));
        } else {
            args.extend(["-c:a", "pcm_s16le"].map(String::from));
        }
        args.push(final_path.to_string_lossy().to_string());
        let rc = run_ffmpeg(handle, app, id, &bins.ffmpeg, &args, segment_duration, "Conversion audio…")
            .map_err(|e| e.to_string())?;
        check_cancel(handle)?;
        if rc != 0 {
            return Err("La conversion audio a échoué.".into());
        }
        let _ = std::fs::remove_file(&src);
    } else {
        // ---- Video ----
        status(
            app,
            id,
            if has_range {
                "Téléchargement (segment découpé ensuite)…"
            } else {
                "Téléchargement…"
            },
            "info",
        );
        let temp_base = format!("temp_{video_id}");

        // Telechargement (avec repli auto 1080p si la qualite max echoue).
        let run_dl = |fmt: &str| -> Result<(i32, String), String> {
            let mut cmd = Command::new(&bins.ytdlp);
            base_ytdlp_args(&mut cmd, bins, remote);
            cmd.arg("-f").arg(fmt).arg("-S").arg(FORMAT_SORT);
            if native_video {
                cmd.arg("-o").arg(download_dir.join(format!("{temp_base}.%(ext)s")));
            } else {
                cmd.arg("--merge-output-format")
                    .arg("mp4")
                    .arg("-o")
                    .arg(download_dir.join(format!("{temp_base}.mp4")));
            }
            cmd.arg("--newline")
                .arg("--progress-template")
                .arg("download:RLPROG %(progress.downloaded_bytes)s %(progress.total_bytes)s %(progress.total_bytes_estimate)s");
            add_subs_thumb(&mut cmd, opts.subs, opts.thumb);
            apply_cookies(&mut cmd, &used_cookie);
            cmd.arg(url);
            run_ytdlp_download(handle, app, id, cmd, "Téléchargement…").map_err(|e| e.to_string())
        };

        let cleanup_partial = || {
            for p in files_with_prefix(download_dir, &temp_base) {
                let _ = std::fs::remove_file(p);
            }
        };

        let (mut code, mut err) = run_dl(&dl_format)?;
        if code != 0 && !handle.cancelled() && dl_format != fallback_format {
            cleanup_partial();
            status(app, id, "Qualité max indisponible — repli en 1080p…", "warn");
            let r = run_dl(&fallback_format)?;
            code = r.0;
            err = r.1;
        }
        check_cancel(handle)?;
        if code != 0 {
            return Err(if err.trim().is_empty() {
                "Téléchargement échoué.".into()
            } else {
                err.trim().lines().last().unwrap_or("Téléchargement échoué.").to_string()
            });
        }

        if native_video {
            let temp_file = files_with_prefix(download_dir, &temp_base)
                .into_iter()
                .find(|p| {
                    let e = ext_lower(p);
                    e != "part" && !["srt", "jpg", "jpeg", "png", "webp"].contains(&e.as_str())
                })
                .ok_or("Fichier téléchargé introuvable.")?;
            let actual_ext = ext_lower(&temp_file);
            final_path = download_dir.join(format!("{safe_title}.{actual_ext}"));
            if has_range {
                status(app, id, "Découpe du segment (copie flux)…", "info");
                let mut args: Vec<String> = vec!["-y".into(), "-progress".into(), "pipe:1".into()];
                args.extend(seek.clone());
                args.push("-i".into());
                args.push(temp_file.to_string_lossy().to_string());
                args.extend(dur.clone());
                args.extend(["-c", "copy"].map(String::from));
                args.push(final_path.to_string_lossy().to_string());
                let rc = run_ffmpeg(handle, app, id, &bins.ffmpeg, &args, segment_duration, "Découpe…")
                    .map_err(|e| e.to_string())?;
                if rc != 0 {
                    return Err("La découpe du segment a échoué.".into());
                }
                let _ = std::fs::remove_file(&temp_file);
            } else {
                status(app, id, "Finalisation…", "info");
                let _ = std::fs::remove_file(&final_path);
                std::fs::rename(&temp_file, &final_path).map_err(|e| e.to_string())?;
            }
        } else {
            let temp_file = download_dir.join(format!("{temp_base}.mp4"));
            let codec = probe_video_codec(&bins.ffprobe, &temp_file);
            let needs_transcode = want_prores || !PREMIERE_READY_CODECS.contains(&codec.as_str());
            let ext = if want_prores { "mov" } else { "mp4" };
            final_path = download_dir.join(format!("{safe_title}.{ext}"));

            if !needs_transcode && !has_range {
                status(app, id, "Finalisation…", "info");
                let _ = std::fs::remove_file(&final_path);
                std::fs::rename(&temp_file, &final_path).map_err(|e| e.to_string())?;
            } else if !needs_transcode && has_range {
                // Decoupe PRECISE en H.264 (le 'copy' deborde sur la keyframe).
                status(app, id, "Découpe du segment…", "info");
                let mut args: Vec<String> = vec!["-y".into(), "-progress".into(), "pipe:1".into()];
                args.extend(seek.clone());
                args.push("-i".into());
                args.push(temp_file.to_string_lossy().to_string());
                args.extend(dur.clone());
                args.extend(
                    [
                        "-c:v", "libx264", "-crf", "18", "-preset", "fast", "-pix_fmt", "yuv420p",
                        "-c:a", "aac", "-b:a", "256k", "-movflags", "+faststart",
                    ]
                    .map(String::from),
                );
                args.push(final_path.to_string_lossy().to_string());
                let rc = run_ffmpeg(handle, app, id, &bins.ffmpeg, &args, segment_duration, "Découpe…")
                    .map_err(|e| e.to_string())?;
                if rc != 0 {
                    return Err("La découpe du segment a échoué.".into());
                }
                let _ = std::fs::remove_file(&temp_file);
            } else {
                // Transcodage (GPU puis repli CPU), comme le Python.
                let is_mac = cfg!(target_os = "macos");
                let (label, venc, aenc, venc_cpu, aenc_cpu): (
                    &str,
                    Vec<&str>,
                    Vec<&str>,
                    Vec<&str>,
                    Vec<&str>,
                ) = if want_prores {
                    (
                        "Conversion ProRes…",
                        if is_mac {
                            vec!["-c:v", "prores_videotoolbox", "-profile:v", "3"]
                        } else {
                            vec!["-c:v", "prores_ks", "-profile:v", "3"]
                        },
                        vec!["-c:a", "pcm_s16le"],
                        vec!["-c:v", "prores_ks", "-profile:v", "3"],
                        vec!["-c:a", "pcm_s16le"],
                    )
                } else {
                    (
                        "Conversion H.265 (Premiere Pro)…",
                        if is_mac {
                            vec!["-c:v", "hevc_videotoolbox", "-q:v", "65", "-tag:v", "hvc1"]
                        } else {
                            vec![
                                "-c:v", "hevc_nvenc", "-rc", "vbr", "-cq", "18", "-pix_fmt",
                                "yuv420p", "-tag:v", "hvc1",
                            ]
                        },
                        vec!["-c:a", "aac", "-b:a", "256k"],
                        vec!["-c:v", "libx265", "-crf", "18", "-preset", "fast", "-tag:v", "hvc1"],
                        vec!["-c:a", "aac", "-b:a", "256k"],
                    )
                };

                status(app, id, label, "info");
                emit(app, TaskUpdate { id, percent: Some(0.0), ..Default::default() });
                let build = |venc: &[&str], aenc: &[&str]| -> Vec<String> {
                    let mut a: Vec<String> = vec!["-y".into(), "-progress".into(), "pipe:1".into()];
                    a.extend(seek.clone());
                    a.push("-i".into());
                    a.push(temp_file.to_string_lossy().to_string());
                    a.extend(dur.clone());
                    a.extend(venc.iter().map(|s| s.to_string()));
                    a.extend(aenc.iter().map(|s| s.to_string()));
                    a.push(final_path.to_string_lossy().to_string());
                    a
                };
                let rc = run_ffmpeg(handle, app, id, &bins.ffmpeg, &build(&venc, &aenc), segment_duration, label)
                    .map_err(|e| e.to_string())?;
                check_cancel(handle)?;
                if rc != 0 {
                    status(app, id, "Encodage CPU (secours)…", "warn");
                    emit(app, TaskUpdate { id, percent: Some(0.0), ..Default::default() });
                    let rc2 = run_ffmpeg(
                        handle,
                        app,
                        id,
                        &bins.ffmpeg,
                        &build(&venc_cpu, &aenc_cpu),
                        segment_duration,
                        "Encodage CPU…",
                    )
                    .map_err(|e| e.to_string())?;
                    check_cancel(handle)?;
                    if rc2 != 0 {
                        return Err("Le transcodage vidéo a échoué.".into());
                    }
                }
                let _ = std::fs::remove_file(&temp_file);
            }
        }
    }

    // Deplacer sous-titres / miniature (ecrits a cote de temp_<id>) vers le nom final.
    if (opts.subs || opts.thumb) && !subs_only {
        let src_prefix = format!("temp_{video_id}");
        let dst_base = final_path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| safe_title.clone());
        for p in files_with_prefix(download_dir, &src_prefix) {
            let e = ext_lower(&p);
            if !["srt", "jpg", "jpeg", "png", "webp"].contains(&e.as_str()) {
                continue;
            }
            if let Some(name) = p.file_name().and_then(|n| n.to_str()) {
                let suffix = &name[src_prefix.len()..]; // ex ".fr.srt" / ".jpg"
                let _ = std::fs::rename(&p, download_dir.join(format!("{dst_base}{suffix}")));
            }
        }
    }

    let done_msg = if subs_only {
        "Terminé ✓ Sous-titres (.srt)".to_string()
    } else if audio_mode {
        format!("Terminé ✓ Audio {}", audio_codec.to_uppercase())
    } else if want_prores {
        "Terminé ✓ ProRes (.mov)".to_string()
    } else if native_video {
        format!("Terminé ✓ Vidéo native (.{})", ext_lower(&final_path))
    } else {
        "Terminé ✓ Prêt pour Premiere Pro".to_string()
    };
    let _ = dir_str; // (garde la coherence avec la version Python)
    emit(
        app,
        TaskUpdate {
            id,
            status: Some(done_msg),
            status_kind: Some("ok".into()),
            percent: Some(1.0),
            indeterminate: Some(false),
            action: Some("open".into()),
            final_path: Some(final_path.to_string_lossy().to_string()),
            done: Some(true),
            ..Default::default()
        },
    );
    Ok(())
}

// ============================================================
//  Commandes Tauri
// ============================================================

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvInfo {
    download_dir: String,
    cookies_ok: bool,
    cookies_source: String,
    js_runtime: bool,
}

#[derive(Deserialize, Serialize, Default)]
struct PersistConfig {
    #[serde(default)]
    download_dir: Option<String>,
}

fn config_file() -> PathBuf {
    config_dir().join("config.json")
}

fn load_config() -> PersistConfig {
    std::fs::read_to_string(config_file())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_config(cfg: &PersistConfig) {
    let _ = std::fs::create_dir_all(config_dir());
    if let Ok(s) = serde_json::to_string(cfg) {
        let _ = std::fs::write(config_file(), s);
    }
}

#[tauri::command]
pub fn get_env(app: AppHandle) -> EnvInfo {
    let cfg = load_config();
    let download_dir = cfg
        .download_dir
        .filter(|d| !d.is_empty() && Path::new(d).is_dir())
        .unwrap_or_else(|| default_downloads_dir().to_string_lossy().to_string());

    let (cookies_ok, cookies_source) = if cookie_path(&app).is_some() {
        (true, "cookies.txt".to_string())
    } else {
        let b = detect_browsers();
        if b.is_empty() {
            (false, String::new())
        } else {
            (true, b.join("/"))
        }
    };

    EnvInfo {
        download_dir,
        cookies_ok,
        cookies_source,
        js_runtime: has_js_runtime(),
    }
}

#[tauri::command]
pub fn set_download_dir(dir: String) {
    let mut cfg = load_config();
    cfg.download_dir = Some(dir);
    save_config(&cfg);
}

#[tauri::command]
pub async fn choose_destination(app: AppHandle) -> Option<String> {
    use tauri_plugin_dialog::DialogExt;
    let cfg = load_config();
    let start = cfg
        .download_dir
        .unwrap_or_else(|| default_downloads_dir().to_string_lossy().to_string());
    let (tx, rx) = std::sync::mpsc::channel();
    app.dialog()
        .file()
        .set_directory(start)
        .pick_folder(move |f| {
            let _ = tx.send(f);
        });
    let picked = rx.recv().ok().flatten();
    if let Some(ref p) = picked {
        let s = p.to_string();
        let mut cfg = load_config();
        cfg.download_dir = Some(s.clone());
        save_config(&cfg);
        return Some(s);
    }
    None
}

#[tauri::command]
pub fn start_download(app: AppHandle, state: State<Engine>, opts: DownloadOpts) {
    let handle = TaskHandle::new();
    state.tasks.lock().unwrap().insert(opts.id, handle.clone());
    let bins = Bins {
        ytdlp: resolve_binary(&app, "yt-dlp"),
        ffmpeg: resolve_binary(&app, "ffmpeg"),
        ffprobe: resolve_binary(&app, "ffprobe"),
    };
    let tasks = state.tasks.clone();
    let opts2 = opts.clone();
    std::thread::spawn(move || {
        download_pipeline(&app, &handle, &opts2, &bins);
        tasks.lock().unwrap().remove(&opts2.id);
    });
}

#[tauri::command]
pub fn cancel_download(state: State<Engine>, id: u64) {
    if let Some(h) = state.tasks.lock().unwrap().get(&id) {
        h.cancel.store(true, Ordering::SeqCst);
        if let Some(c) = h.child.lock().unwrap().as_mut() {
            let _ = c.kill();
        }
    }
}

#[tauri::command]
pub fn reveal_in_folder(path: String) {
    let p = Path::new(&path);
    #[cfg(windows)]
    {
        let mut cmd = Command::new("explorer");
        cmd.arg("/select,").arg(&path);
        no_window(&mut cmd);
        let _ = cmd.spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = Command::new("open").arg("-R").arg(&path).spawn();
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let dir = p.parent().unwrap_or(p);
        let _ = Command::new("xdg-open").arg(dir).spawn();
    }
}

#[tauri::command]
pub fn open_cookie_help() {
    open_url(COOKIE_FIX_URL);
}

fn open_url(url: &str) {
    #[cfg(windows)]
    {
        let mut cmd = Command::new("cmd");
        cmd.args(["/C", "start", "", url]);
        no_window(&mut cmd);
        let _ = cmd.spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = Command::new("open").arg(url).spawn();
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let _ = Command::new("xdg-open").arg(url).spawn();
    }
}
