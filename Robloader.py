import os
import sys
import json
import shutil
import ssl
import tempfile
import threading
import subprocess
import urllib.request
import webbrowser

# Marche a suivre cookies (affichee par le bouton "Reparer" quand YouTube exige une connexion).
COOKIE_FIX_URL = "https://docs.google.com/document/d/1zCuLswlQeOCV-C7bQWlmi6Ix-OcmPy252RCZSjAeKF4/"

# Version courante de l'app (a bumper a CHAQUE release, en phase avec le tag git vX.Y.Z).
APP_VERSION = "1.0.7"

# Verification de mise a jour : le repo est PUBLIC, donc l'API GitHub Releases est lisible sans
# aucune authentification (ni token embarque). On compare le dernier tag a APP_VERSION et, si plus
# recent, on propose le telechargement+lancement de l'installeur de la plateforme.
GITHUB_REPO = "Splainte/Robloader"
RELEASES_API_URL = f"https://api.github.com/repos/{GITHUB_REPO}/releases/latest"
RELEASES_PAGE_URL = f"https://github.com/{GITHUB_REPO}/releases/latest"
# Nom EXACT de l'asset installeur par plateforme (cf .github/workflows/build.yml).
UPDATE_ASSET = {'win': 'Robloader-Setup.exe', 'darwin': 'Robloader-macos.dmg'}


def config_dir():
    if sys.platform.startswith('win'):
        base = os.environ.get('APPDATA') or os.path.expanduser('~')
    elif sys.platform == 'darwin':
        base = os.path.join(os.path.expanduser('~'), 'Library', 'Application Support')
    else:
        base = os.environ.get('XDG_CONFIG_HOME') or os.path.join(os.path.expanduser('~'), '.config')
    return os.path.join(base, 'Robloader')


def default_downloads_dir():
    """VRAI dossier 'Telechargements' de l'utilisateur — celui que l'OS connait, MEME s'il a ete
    deplace sur un autre disque/partition. On NE reconstruit PAS '~/Downloads' a la main : ca rate
    le dossier relocalise et fabrique un doublon (bug remonte par Robin)."""
    if sys.platform.startswith('win'):
        try:
            return _windows_downloads_dir()
        except Exception:
            pass
    elif sys.platform == 'darwin':
        # macOS ne relocalise pas Downloads : ~/Downloads est toujours le bon chemin
        # (le libelle "Telechargements" n'est qu'un affichage localise du Finder).
        d = os.path.join(os.path.expanduser('~'), 'Downloads')
        if os.path.isdir(d):
            return d
    else:
        # Linux : respecter xdg-user-dirs (le dossier peut etre ailleurs / dans une autre langue).
        try:
            d = subprocess.check_output(['xdg-user-dir', 'DOWNLOAD'], text=True).strip()
            if d and os.path.isdir(d):
                return d
        except Exception:
            pass
    # Repli universel.
    return os.path.join(os.path.expanduser('~'), 'Downloads')


def _windows_downloads_dir():
    """Interroge l'API Windows (FOLDERID_Downloads via SHGetKnownFolderPath) -> chemin REEL du
    dossier Telechargements, relocalisation sur un autre disque incluse. Repli sur le registre
    (User Shell Folders) puis ~/Downloads."""
    import ctypes
    from ctypes import wintypes

    class GUID(ctypes.Structure):
        _fields_ = [("Data1", wintypes.DWORD), ("Data2", wintypes.WORD),
                    ("Data3", wintypes.WORD), ("Data4", ctypes.c_ubyte * 8)]

    # FOLDERID_Downloads = {374DE290-123F-4565-9164-39C4925E467B}
    folderid = GUID(0x374DE290, 0x123F, 0x4565,
                    (ctypes.c_ubyte * 8)(0x91, 0x64, 0x39, 0xC4, 0x92, 0x5E, 0x46, 0x7B))
    ptr = ctypes.c_wchar_p()
    if ctypes.windll.shell32.SHGetKnownFolderPath(
            ctypes.byref(folderid), 0, None, ctypes.byref(ptr)) == 0 and ptr.value:
        path = ptr.value
        ctypes.windll.ole32.CoTaskMemFree(ptr)
        if os.path.isdir(path):
            return path

    # Repli registre : valeur nommee = le GUID Downloads (souvent REG_EXPAND_SZ -> expandvars).
    try:
        import winreg
        sub = r"Software\Microsoft\Windows\CurrentVersion\Explorer\User Shell Folders"
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, sub) as k:
            val, _ = winreg.QueryValueEx(k, "{374DE290-123F-4565-9164-39C4925E467B}")
            path = os.path.expandvars(val)
            if os.path.isdir(path):
                return path
    except Exception:
        pass

    return os.path.join(os.path.expanduser('~'), 'Downloads')


# --- SSL : certifi pour les apps PyInstaller macOS ---
# Sur macOS, le Python bundlé par PyInstaller ne trouve pas les certificats système -> urlopen
# lève SSLCertVerificationError (avalé silencieusement) -> update check / yt-dlp update silencieux.
# certifi fournit un bundle de certs CA indépendant du système.
def _ssl_context():
    try:
        import certifi
        return ssl.create_default_context(cafile=certifi.where())
    except Exception:
        return ssl.create_default_context()

# --- Auto-update yt-dlp ---
# YouTube casse l'extraction regulierement. On charge une version a jour de yt-dlp si elle a ete
# telechargee au lancement PRECEDENT (le wheel PyPI est un zip importable via sys.path). Ainsi
# l'app reste fonctionnelle sans rebuild. Telechargement en tache de fond (cf update_ytdlp_async).
_YTDLP_WHL = os.path.join(config_dir(), 'yt-dlp.whl')
if os.path.exists(_YTDLP_WHL):
    sys.path.insert(0, _YTDLP_WHL)

import tkinter as tk
import customtkinter as ctk
from tkinter import filedialog
import yt_dlp


def _norm_version(v):
    try:
        return tuple(int(x) for x in str(v).split('.'))
    except Exception:
        return (str(v),)


def update_ytdlp_async():
    """En tache de fond : si une version plus recente de yt-dlp existe sur PyPI, telecharge son
    wheel dans le dossier de config -> pris en compte au PROCHAIN lancement (jamais de swap a chaud)."""
    def work():
        try:
            with urllib.request.urlopen('https://pypi.org/pypi/yt-dlp/json', timeout=12, context=_ssl_context()) as r:
                data = json.load(r)
            latest = data['info']['version']
            current = getattr(yt_dlp.version, '__version__', '')
            if latest and _norm_version(latest) > _norm_version(current):
                whl_url = next((f['url'] for f in data['releases'].get(latest, [])
                                if f['filename'].endswith('py3-none-any.whl')), None)
                if whl_url:
                    d = config_dir()
                    os.makedirs(d, exist_ok=True)
                    tmp = os.path.join(d, 'yt-dlp.whl.part')
                    urllib.request.urlretrieve(whl_url, tmp)
                    os.replace(tmp, os.path.join(d, 'yt-dlp.whl'))
        except Exception:
            pass
    threading.Thread(target=work, daemon=True).start()

# --- Strategie d'extraction YouTube (2026) ---
# La 4K est un casse-tete car YouTube applique DEUX protections selon le client :
#   - web / web_embedded : servent la 4K mais exigent un PO Token -> HTTP 403 sur IP residentielle
#                          SAUF si on est authentifie (cookies) -> alors la 4K passe.
#   - tv                 : 4K sans PO Token, MAIS certaines sessions tombent dans une experimentation
#                          YouTube qui colle du DRM a tout sur tv (issue yt-dlp #12563) -> 4K KO.
#   - ios                : filet de secours (mobile, ni PoT ni DRM, mais pas de 4K).
# Pour une 4K FIABLE sur une session filtree : des cookies (navigateur ou cookies.txt) -> le client
# web_embedded delivre alors la 4K sans 403.
#
# PIEGE 'missing_pot' (corrige le 2026-06-08, video 2s_WoPudEKY) : sur certaines videos YouTube
# applique le PO Token a l'AUDIO de web_embedded/tv/ios. Avec 'missing_pot' on GARDAIT ces formats
# audio morts -> bv*+ba telechargeait la video puis l'audio plantait en 403 -> repli 1080p -> rebelote.
# Resultat visible : 2 fichiers temporaires video (4K webm + 1080p mp4) SANS son, et un faux message
# "probleme de cookies". -> On RETIRE 'missing_pot' (les formats morts sont jetes) ET on ajoute le
# client 'android_vr' (audio+video sans PoT ni DRM) + le set 'default' de yt-dlp (qui suit les
# contournements a jour). web_embedded reste pour la 4K-avec-cookies. (cf yt-dlp #12563 / PoT)
YOUTUBE_EXTRACTOR_ARGS = {
    'youtube': {
        'player_client': ['default', 'android_vr', 'web_embedded'],
    }
}

# Resolution du nsig (necessaire pour la 4K/1440p) : yt-dlp execute le vrai JS de YouTube via Deno
# + un script solveur "EJS" telecharge une fois. Sans Deno -> on plafonne a 1080p.
EJS_REMOTE_COMPONENTS = ['ejs:github']

# Tri des formats : meilleure resolution, puis on PREFERE le H.264 (avc) et l'audio AAC. Resultat :
# en 1080p et moins on obtient du MP4/H.264 deja pret pour Premiere (-> pas de transcodage), et la
# 4K/1440p (seulement en VP9/AV1) sera transcodee en H.265.
FORMAT_SORT = ['res', 'fps', 'vcodec:h264', 'acodec:aac', 'br']

# Codecs deja confortables pour Premiere Pro (pas besoin de transcoder en H.265).
PREMIERE_READY_CODECS = ('h264', 'avc1', 'avc', 'hevc', 'h265', 'hev1', 'hvc1')

# Choix de qualite proposes (label -> hauteur max ; None = sans plafond).
QUALITY_OPTIONS = [
    ("Qualité max (jusqu'à 4K)", None),
    ("1440p (2K)", 1440),
    ("1080p (Full HD)", 1080),
    ("720p (HD)", 720),
    ("480p", 480),
]
QUALITY_LABELS = [label for label, _ in QUALITY_OPTIONS]
QUALITY_MAP = dict(QUALITY_OPTIONS)
DEFAULT_QUALITY = QUALITY_LABELS[0]

# Format de sortie.
OUT_HEVC = "HEVC"
OUT_PRORES = "ProRes"
OUT_VIDEO_NATIVE = "Vidéo (natif)"
OUT_MP3 = "Audio MP3"
OUT_WAV = "Audio WAV"
OUT_SUBS = "Sous-titres seuls (.srt)"
OUTPUT_FORMATS_TRANSCODE = [OUT_HEVC, OUT_PRORES, OUT_MP3, OUT_WAV, OUT_SUBS]
OUTPUT_FORMATS_NO_TRANSCODE = [OUT_VIDEO_NATIVE, OUT_MP3, OUT_WAV, OUT_SUBS]
OUTPUT_FORMATS = OUTPUT_FORMATS_TRANSCODE
DEFAULT_OUTPUT = OUT_HEVC


def format_for_height(h):
    """Selecteur de format yt-dlp pour une hauteur max donnee (None = sans plafond)."""
    if not h:
        return 'bv*+ba/b'
    return f'bv*[height<={h}]+ba/bv*[height<={h}]/b[height<={h}]/b'


def load_config():
    try:
        with open(os.path.join(config_dir(), 'config.json'), 'r', encoding='utf-8') as f:
            return json.load(f)
    except Exception:
        return {}


def save_config(cfg):
    try:
        d = config_dir()
        os.makedirs(d, exist_ok=True)
        with open(os.path.join(d, 'config.json'), 'w', encoding='utf-8') as f:
            json.dump(cfg, f)
    except Exception:
        pass


def resource_search_dirs(app_dir):
    """Dossiers ou chercher les fichiers fournis par l'utilisateur (cookies.txt, deno, logo).
    Sous macOS, dans un .app, 'a cote de l'application' est HORS du bundle."""
    dirs = [app_dir]
    if getattr(sys, 'frozen', False) and sys.platform == 'darwin':
        p = sys.executable
        for _ in range(4):  # .../Robloader.app/Contents/MacOS/<exe> -> dossier du .app
            p = os.path.dirname(p)
        dirs.append(p)
        dirs.append(os.path.join(os.path.expanduser("~"), "Library", "Application Support", "Robloader"))
    dirs.append(os.path.expanduser("~"))
    seen, out = set(), []
    for d in dirs:
        if d and d not in seen:
            seen.add(d)
            out.append(d)
    return out


def has_js_runtime():
    """Deno present ? -> requis par yt-dlp pour resoudre le nsig (donc pour la 4K)."""
    return shutil.which('deno') is not None


def detect_browsers():
    """Liste ORDONNEE des navigateurs presents (pour 'cookiesfrombrowser'), a essayer tour a tour.
    Firefox est mis avant Chrome car Chrome VERROUILLE sa base de cookies quand il est ouvert
    (yt-dlp #7271 / #7271 'Could not copy Chrome cookie database') -> Firefox passe meme ouvert."""
    home = os.path.expanduser("~")
    if sys.platform == 'darwin':
        cand = [
            ('firefox', "Library/Application Support/Firefox/Profiles"),
            ('chrome', "Library/Application Support/Google/Chrome"),
            ('brave',  "Library/Application Support/BraveSoftware/Brave-Browser"),
            ('edge',   "Library/Application Support/Microsoft Edge"),
            ('safari', "Library/Containers/com.apple.Safari/Data/Library/Cookies/Cookies.binarycookies"),
        ]
    elif sys.platform.startswith('win'):
        la = os.environ.get('LOCALAPPDATA', '')
        ap = os.environ.get('APPDATA', '')
        cand = [
            ('firefox', os.path.join(ap, "Mozilla", "Firefox", "Profiles")),
            ('chrome', os.path.join(la, "Google", "Chrome", "User Data")),
            ('edge',   os.path.join(la, "Microsoft", "Edge", "User Data")),
            ('brave',  os.path.join(la, "BraveSoftware", "Brave-Browser", "User Data")),
        ]
    else:
        cand = [
            ('firefox', os.path.join(home, ".mozilla", "firefox")),
            ('chrome', os.path.join(home, ".config", "google-chrome")),
        ]
    found = []
    for name, rel in cand:
        p = rel if os.path.isabs(rel) else os.path.join(home, rel)
        if os.path.exists(p):
            found.append(name)
    return found


def pick_writable_tempdir(preferred):
    """Dossier temporaire REELLEMENT inscriptible (sous Windows lance en admin, le temp peut
    pointer sur C:\\Windows\\system32 -> yt-dlp/Deno n'y ecrivent pas -> 4K KO). Test par ecriture."""
    home = os.path.expanduser("~")
    local_appdata = os.environ.get("LOCALAPPDATA")
    candidates = [
        tempfile.gettempdir(),
        os.path.join(local_appdata, "Temp") if local_appdata else None,
        os.path.join(home, "AppData", "Local", "Temp"),
        preferred,
        home,
    ]
    for d in candidates:
        if not d:
            continue
        try:
            os.makedirs(d, exist_ok=True)
            probe = tempfile.NamedTemporaryFile(dir=d, delete=True)
            probe.close()
            return d
        except Exception:
            continue
    return home


# Couleurs
ACCENT = "#1f6aa5"
ACCENT_HOVER = "#175384"
OK_GREEN = "#2ecc71"
WARN_ORANGE = "#e67e22"
ERR_RED = "#e74c3c"
MUTED = "#9aa0a6"
CARD = "#2b2b2b"


class RobloaderApp(ctk.CTk):
    def __init__(self):
        super().__init__()

        ctk.set_appearance_mode("dark")
        ctk.set_default_color_theme("blue")

        self.title("Robloader")
        self.geometry("840x640")
        self.minsize(760, 560)

        self.task_counter = 0
        self.active_tasks = {}
        self.task_widgets = {}   # task_id -> frame (pour le bouton Nettoyer)

        self.config_data = load_config()

        # --- Chemins de base ---
        if getattr(sys, 'frozen', False):
            self.ffmpeg_base_dir = sys._MEIPASS
            self.app_dir = os.path.dirname(sys.executable)
        else:
            self.ffmpeg_base_dir = os.path.dirname(os.path.abspath(__file__))
            self.app_dir = self.ffmpeg_base_dir

        is_win = sys.platform.startswith("win")
        ffmpeg_name = "ffmpeg.exe" if is_win else "ffmpeg"
        ffprobe_name = "ffprobe.exe" if is_win else "ffprobe"

        # PATH : binaires embarques + dossiers ou l'utilisateur peut poser deno/ffmpeg.
        self.search_dirs = resource_search_dirs(self.app_dir)
        for d in [self.ffmpeg_base_dir] + self.search_dirs:
            if d:
                os.environ["PATH"] += os.pathsep + d

        self.ffmpeg_path = self._resolve_binary(ffmpeg_name)
        self.ffprobe_path = self._resolve_binary(ffprobe_name)

        self.js_runtime = has_js_runtime()
        self.browsers = detect_browsers()

        # cookies.txt : 1er trouve (prioritaire sur les cookies navigateur).
        self.cookie_path = os.path.join(self.app_dir, "cookies.txt")
        for d in self.search_dirs:
            c = os.path.join(d, "cookies.txt")
            if os.path.exists(c):
                self.cookie_path = c
                break

        # Dossier de destination : dernier utilise (memorise) sinon le VRAI dossier Telechargements
        # de l'OS (relocalisation sur un autre disque incluse — cf default_downloads_dir).
        saved_dir = self.config_data.get("download_dir")
        self.download_dir = saved_dir if (saved_dir and os.path.isdir(saved_dir)) \
            else default_downloads_dir()

        self.temp_dir = pick_writable_tempdir(self.download_dir)
        tempfile.tempdir = self.temp_dir
        os.environ["TEMP"] = self.temp_dir
        os.environ["TMP"] = self.temp_dir

        self._set_window_icon()
        self._build_ui()

        # Met yt-dlp a jour en arriere-plan (effectif au prochain lancement).
        update_ytdlp_async()
        # Verifie en arriere-plan si une nouvelle version de Robloader est disponible.
        self._check_update_async()

    # ---------- Helpers ----------
    def _resolve_binary(self, name):
        for d in [self.ffmpeg_base_dir] + self.search_dirs:
            cand = os.path.join(d, name)
            if os.path.exists(cand):
                return cand
        return shutil.which(name) or name

    def _find_asset(self, name):
        for d in [self.ffmpeg_base_dir] + self.search_dirs:
            p = os.path.join(d, name)
            if os.path.exists(p):
                return p
        return None

    def _set_window_icon(self):
        png = self._find_asset("logo.png")
        if png:
            try:
                self._icon_img = tk.PhotoImage(file=png)
                self.iconphoto(True, self._icon_img)
            except Exception:
                pass

    def ui(self, fn):
        # Tkinter n'est pas thread-safe : toute MAJ depuis un thread de travail passe par after().
        try:
            self.after(0, fn)
        except Exception:
            pass

    # ---------- Construction de l'interface ----------
    def _build_ui(self):
        # En-tete
        header = ctk.CTkFrame(self, fg_color="transparent")
        header.pack(fill="x", padx=24, pady=(18, 4))
        self._header = header
        ctk.CTkLabel(header, text="Robloader", font=("Helvetica", 24, "bold")).pack(side="left")
        ctk.CTkLabel(header, text="  YouTube → fichier prêt pour Premiere Pro",
                     font=("Helvetica", 12), text_color=MUTED).pack(side="left", pady=(8, 0))
        ctk.CTkLabel(header, text=f"v{APP_VERSION}", font=("Helvetica", 11),
                     text_color=MUTED).pack(side="right", pady=(10, 0))

        # Banniere de mise a jour : creee masquee, affichee (apres l'en-tete) si _check_update_async
        # detecte une version plus recente. "Telecharger" ouvre la Release ; "Plus tard" la masque.
        self.update_banner = ctk.CTkFrame(self, fg_color="#1f3a2e")
        self.update_lbl = ctk.CTkLabel(self.update_banner, text="", text_color=OK_GREEN,
                                       font=("Helvetica", 12, "bold"), anchor="w")
        self.update_lbl.pack(side="left", padx=12, pady=6)
        ctk.CTkButton(self.update_banner, text="Plus tard", width=80, height=28,
                      fg_color="#3a3a3a", hover_color="#4a4a4a",
                      command=self.update_banner.pack_forget).pack(side="right", padx=(6, 12), pady=6)
        ctk.CTkButton(self.update_banner, text="Mettre à jour", width=120, height=28,
                      fg_color=ACCENT, hover_color=ACCENT_HOVER,
                      command=self._start_update).pack(side="right", padx=6, pady=6)

        # Ligne 1 : URL + qualite + destination + telecharger
        row1 = ctk.CTkFrame(self, fg_color="transparent")
        row1.pack(fill="x", padx=24, pady=(8, 4))
        row1.grid_columnconfigure(0, weight=1)

        self.url_entry = ctk.CTkEntry(row1, placeholder_text="Colle un lien YouTube ici…", height=38)
        self.url_entry.grid(row=0, column=0, sticky="ew", padx=(0, 8))
        self.url_entry.bind("<Return>", lambda e: self.start_download_thread())

        self.quality_var = ctk.StringVar(
            value=self.config_data.get("quality", DEFAULT_QUALITY) if
            self.config_data.get("quality", DEFAULT_QUALITY) in QUALITY_LABELS else DEFAULT_QUALITY)
        self.quality_menu = ctk.CTkOptionMenu(
            row1, values=QUALITY_LABELS, variable=self.quality_var, width=170, height=38,
            command=self._on_quality_change, fg_color=CARD, button_color="#3a3a3a",
            button_hover_color="#4a4a4a")
        self.quality_menu.grid(row=0, column=1, padx=4)

        self.folder_btn = ctk.CTkButton(row1, text="Destination", width=110, height=38,
                                        fg_color="#3a3a3a", hover_color="#4a4a4a",
                                        command=self.choose_folder)
        self.folder_btn.grid(row=0, column=2, padx=4)

        self.download_btn = ctk.CTkButton(row1, text="Télécharger", width=130, height=38,
                                         fg_color=ACCENT, hover_color=ACCENT_HOVER,
                                         font=("Helvetica", 13, "bold"),
                                         command=self.start_download_thread)
        self.download_btn.grid(row=0, column=3, padx=(4, 0))

        # Ligne 2 : extrait optionnel
        row2 = ctk.CTkFrame(self, fg_color="transparent")
        row2.pack(fill="x", padx=24, pady=(2, 2))
        ctk.CTkLabel(row2, text="Extrait (optionnel)  ", font=("Helvetica", 12, "bold")).pack(side="left")
        ctk.CTkLabel(row2, text="Début", font=("Helvetica", 11), text_color=MUTED).pack(side="left", padx=(4, 4))
        self.start_entry = ctk.CTkEntry(row2, placeholder_text="00:00", width=78, height=32)
        self.start_entry.pack(side="left", padx=(0, 12))
        ctk.CTkLabel(row2, text="Fin", font=("Helvetica", 11), text_color=MUTED).pack(side="left", padx=(0, 4))
        self.end_entry = ctk.CTkEntry(row2, placeholder_text="01:30", width=78, height=32)
        self.end_entry.pack(side="left", padx=(0, 12))
        ctk.CTkLabel(row2, text="format MM:SS ou HH:MM:SS — laisser vide pour la vidéo entière",
                     font=("Helvetica", 11), text_color=MUTED).pack(side="left")

        # Ligne 3 : transcodage + format de sortie + sous-titres + miniature
        row3 = ctk.CTkFrame(self, fg_color="transparent")
        row3.pack(fill="x", padx=24, pady=(4, 2))
        self.transcode_var = tk.BooleanVar(value=bool(self.config_data.get("transcode", True)))
        ctk.CTkCheckBox(row3, text="Transcodage", variable=self.transcode_var,
                        command=self._on_transcode_change,
                        height=28).pack(side="left", padx=(0, 16))
        ctk.CTkLabel(row3, text="Sortie  ", font=("Helvetica", 12, "bold")).pack(side="left")
        transcode_on = self.transcode_var.get()
        valid_formats = OUTPUT_FORMATS_TRANSCODE if transcode_on else OUTPUT_FORMATS_NO_TRANSCODE
        out_init = self.config_data.get("output", DEFAULT_OUTPUT)
        if out_init not in valid_formats:
            out_init = valid_formats[0]
        self.output_var = ctk.StringVar(value=out_init)
        self.output_menu = ctk.CTkOptionMenu(
            row3, values=valid_formats, variable=self.output_var, width=170, height=32,
            command=lambda v: self._save_pref("output", v),
            fg_color=CARD, button_color="#3a3a3a", button_hover_color="#4a4a4a")
        self.output_menu.pack(side="left", padx=(0, 16))
        self.subs_var = tk.BooleanVar(value=bool(self.config_data.get("subs", False)))
        ctk.CTkCheckBox(row3, text="Sous-titres (.srt)", variable=self.subs_var,
                        command=lambda: self._save_pref("subs", self.subs_var.get()),
                        height=28).pack(side="left", padx=(0, 14))
        self.thumb_var = tk.BooleanVar(value=bool(self.config_data.get("thumb", False)))
        ctk.CTkCheckBox(row3, text="Miniature", variable=self.thumb_var,
                        command=lambda: self._save_pref("thumb", self.thumb_var.get()),
                        height=28).pack(side="left")

        # Ligne d'etat (destination + cookies/deno)
        self.path_label = ctk.CTkLabel(self, text="", font=("Helvetica", 11), text_color=MUTED, anchor="w")
        self.path_label.pack(fill="x", padx=26, pady=(4, 0))
        self._refresh_status_line()

        # En-tete de la file + bouton Nettoyer
        qhead = ctk.CTkFrame(self, fg_color="transparent")
        qhead.pack(fill="x", padx=24, pady=(10, 0))
        ctk.CTkLabel(qhead, text="File de téléchargements", font=("Helvetica", 13, "bold")).pack(side="left")
        self.clean_btn = ctk.CTkButton(qhead, text="Nettoyer la liste", width=130, height=30,
                                       fg_color="#3a3a3a", hover_color="#4a4a4a",
                                       command=self.clear_queue)
        self.clean_btn.pack(side="right")

        self.scroll_frame = ctk.CTkScrollableFrame(self, fg_color="#212121", label_text="")
        self.scroll_frame.pack(padx=24, pady=(6, 18), fill="both", expand=True)

    def _refresh_status_line(self):
        bits = [f"📁 {self.download_dir}"]
        if os.path.exists(self.cookie_path):
            bits.append("cookies.txt ✓")
        elif self.browsers:
            bits.append(f"cookies {'/'.join(self.browsers[:2])} ✓")
        else:
            bits.append("sans cookies (4K limitée)")
        if not self.js_runtime:
            bits.append("Deno absent → 1080p max")
        self.path_label.configure(text="     ".join(bits))

    def _on_quality_change(self, value):
        self.config_data["quality"] = value
        save_config(self.config_data)

    def _save_pref(self, key, value):
        self.config_data[key] = value
        save_config(self.config_data)

    def _on_transcode_change(self):
        transcode = self.transcode_var.get()
        self._save_pref("transcode", transcode)
        current = self.output_var.get()
        if transcode:
            self.output_menu.configure(values=OUTPUT_FORMATS_TRANSCODE)
            if current == OUT_VIDEO_NATIVE:
                self.output_var.set(OUT_HEVC)
                self._save_pref("output", OUT_HEVC)
        else:
            self.output_menu.configure(values=OUTPUT_FORMATS_NO_TRANSCODE)
            if current in (OUT_HEVC, OUT_PRORES):
                self.output_var.set(OUT_VIDEO_NATIVE)
                self._save_pref("output", OUT_VIDEO_NATIVE)

    # ---------- Actions ----------
    def choose_folder(self):
        folder = filedialog.askdirectory(initialdir=self.download_dir)
        if folder:
            self.download_dir = folder
            self.config_data["download_dir"] = folder
            save_config(self.config_data)
            self._refresh_status_line()

    def clear_queue(self):
        # Retire de la liste les taches terminees / echouees / annulees (pas les telechargements en cours).
        for tid in list(self.task_widgets.keys()):
            if tid not in self.active_tasks:
                try:
                    self.task_widgets[tid].destroy()
                except Exception:
                    pass
                self.task_widgets.pop(tid, None)

    def parse_timecode(self, tc_str):
        if not tc_str or not tc_str.strip():
            return None
        parts = tc_str.strip().split(':')
        try:
            if len(parts) == 1:
                return float(parts[0])
            elif len(parts) == 2:
                return int(parts[0]) * 60 + float(parts[1])
            elif len(parts) == 3:
                return int(parts[0]) * 3600 + int(parts[1]) * 60 + float(parts[2])
        except ValueError:
            return None
        return None

    def start_download_thread(self):
        raw = self.url_entry.get().strip()
        if not raw:
            return

        start_val = self.start_entry.get().strip()
        end_val = self.end_entry.get().strip()
        quality_label = self.quality_var.get()
        output = self.output_var.get()
        subs = bool(self.subs_var.get())
        thumb = bool(self.thumb_var.get())
        transcode = bool(self.transcode_var.get())

        self.url_entry.delete(0, "end")
        self.start_entry.delete(0, "end")
        self.end_entry.delete(0, "end")

        # Batch : on accepte plusieurs URLs separees par des espaces / retours a la ligne.
        for url in [u for u in raw.split() if u.startswith("http")] or [raw]:
            self._enqueue(url, start_val, end_val, quality_label, output, subs, thumb, transcode)

    def _enqueue(self, url, start_val, end_val, quality_label, output, subs, thumb, transcode):
        self.task_counter += 1
        task_id = self.task_counter

        task_frame = ctk.CTkFrame(self.scroll_frame, fg_color=CARD, corner_radius=10)
        task_frame.pack(fill="x", pady=6, padx=6)
        self.task_widgets[task_id] = task_frame

        title_label = ctk.CTkLabel(task_frame, text="Analyse du lien…", font=("Helvetica", 13, "bold"), anchor="w")
        title_label.pack(fill="x", padx=12, pady=(8, 0))

        status_label = ctk.CTkLabel(task_frame, text="En attente…", font=("Helvetica", 11), text_color=MUTED, anchor="w")
        status_label.pack(fill="x", padx=12)

        progress_bar = ctk.CTkProgressBar(task_frame, height=10)
        progress_bar.pack(fill="x", padx=12, pady=(6, 8), side="left", expand=True)
        progress_bar.set(0)

        action_btn = ctk.CTkButton(task_frame, text="Annuler", width=120, height=30,
                                   fg_color="#3a3a3a", hover_color="#4a4a4a",
                                   command=lambda: self.cancel_task(task_id))
        action_btn.pack(side="right", padx=12, pady=8)

        self.active_tasks[task_id] = {
            'cancel_requested': False,
            'process': None,
            'action_btn': action_btn,
            'status_label': status_label,
            'progress_bar': progress_bar,
            'temp_output': None,
            'final_output': None,
        }

        thread = threading.Thread(
            target=self.download_pipeline,
            args=(url, start_val, end_val, quality_label, output, subs, thumb, transcode, task_id,
                  title_label, status_label, progress_bar, action_btn))
        thread.daemon = True
        thread.start()

    def cancel_task(self, task_id):
        if task_id in self.active_tasks:
            self.active_tasks[task_id]['cancel_requested'] = True
            self.active_tasks[task_id]['status_label'].configure(text="Annulation…", text_color=WARN_ORANGE)
            process = self.active_tasks[task_id]['process']
            if process:
                try:
                    process.terminate()
                except Exception:
                    pass

    def _cookie_attempts(self):
        """Liste ORDONNEE d'options cookies a essayer jusqu'a ce qu'une marche, puis sans cookies.
        cookies.txt (fichier) d'abord, puis chaque navigateur (Chrome verrouille s'il est ouvert
        -> on enchaine sur Firefox/Edge), et enfin {} (sans cookies) en dernier recours -> jamais
        de plantage, juste une qualite degradee si rien ne marche."""
        attempts = []
        if os.path.exists(self.cookie_path):
            attempts.append({'cookiefile': self.cookie_path})
        for b in self.browsers:
            attempts.append({'cookiesfrombrowser': (b,)})
        attempts.append({})  # sans cookies
        return attempts

    def _probe_video_codec(self, path):
        try:
            out = subprocess.run(
                [self.ffprobe_path, '-v', 'error', '-select_streams', 'v:0',
                 '-show_entries', 'stream=codec_name', '-of', 'default=nw=1:nk=1', path],
                capture_output=True, text=True, timeout=30)
            return (out.stdout or '').strip().lower()
        except Exception:
            return ''

    def run_ffmpeg_with_progress(self, cmd, duration, task_id, status_lbl, prog_bar, step_text):
        startupinfo = None
        if sys.platform.startswith("win"):
            startupinfo = subprocess.STARTUPINFO()
            startupinfo.dwFlags |= subprocess.STARTF_USESHOWWINDOW

        process = subprocess.Popen(
            cmd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
            universal_newlines=True, encoding='utf-8', startupinfo=startupinfo)
        self.active_tasks[task_id]['process'] = process

        last_percent = -1
        for line in process.stdout:
            if self.active_tasks[task_id]['cancel_requested']:
                process.terminate()
                break
            if 'out_time_us=' in line:
                try:
                    time_us = int(line.split('=')[1].strip())
                    if duration > 0:
                        percent = min(max(time_us / (duration * 1000000), 0.0), 1.0)
                        if int(percent * 100) != last_percent:
                            last_percent = int(percent * 100)
                            self.ui(lambda p=percent: prog_bar.set(p))
                            self.ui(lambda p=percent: status_lbl.configure(text=f"{step_text} {int(p * 100)}%"))
                except Exception:
                    pass
        process.wait()
        return process.returncode

    def download_pipeline(self, url, start_str, end_str, quality_label, output, subs, thumb, transcode, task_id,
                          title_lbl, status_lbl, prog_bar, action_btn):
        temp_output = None
        final_output = None
        try:
            start_seconds = self.parse_timecode(start_str)
            end_seconds = self.parse_timecode(end_str)

            # Hauteur cible selon le choix utilisateur ; sans Deno on plafonne a 1080p (nsig requis au-dela).
            target_h = QUALITY_MAP.get(quality_label)
            if not self.js_runtime:
                target_h = min(target_h or 1080, 1080)
            dl_format = format_for_height(target_h)
            fb_h = 1080 if not target_h else min(target_h, 1080)
            fallback_format = format_for_height(fb_h)
            dl_remote = EJS_REMOTE_COMPONENTS if self.js_runtime else []
            audio_mode = output in (OUT_MP3, OUT_WAV)
            want_prores = (output == OUT_PRORES)
            subs_only = (output == OUT_SUBS)
            native_video = (output == OUT_VIDEO_NATIVE) or (not transcode and not audio_mode and not subs_only and not want_prores and output != OUT_HEVC)
            audio_codec = 'mp3' if output == OUT_MP3 else 'wav'

            # Options sous-titres / miniature, INTEGREES au telechargement principal (ecrites a cote
            # du fichier pendant le DL valide -> bien plus fiable qu'un post-traitement separe).
            extra_opts = {}
            extra_pps = []
            if subs:
                extra_opts.update({'writesubtitles': True, 'writeautomaticsub': True,
                                   'subtitleslangs': ['fr', 'en'], 'subtitlesformat': 'srt/best'})
                extra_pps.append({'key': 'FFmpegSubtitlesConvertor', 'format': 'srt'})
            if thumb:
                extra_opts['writethumbnail'] = True
                extra_pps.append({'key': 'FFmpegThumbnailsConvertor', 'format': 'jpg'})

            self.ui(lambda: status_lbl.configure(text="Analyse de la vidéo…", text_color=MUTED))

            info_opts = {
                'quiet': True, 'noplaylist': True, 'extractor_args': YOUTUBE_EXTRACTOR_ARGS,
                'remote_components': dl_remote, 'format': dl_format, 'format_sort': FORMAT_SORT,
            }

            # Robustesse cookies : on essaie chaque source de cookies (fichier, puis navigateurs)
            # jusqu'a ce qu'une marche, et SANS cookies en dernier recours. Gere Chrome verrouille
            # (yt-dlp #7271) -> bascule sur Firefox/Edge, sinon 1080p sans cookies au lieu de planter.
            cookie_opts = {}
            info = None
            last_err = None
            for i, attempt in enumerate(self._cookie_attempts()):
                if self.active_tasks[task_id]['cancel_requested']:
                    raise Exception("Annulé par l'utilisateur")
                if i > 0 and attempt == {}:
                    self.ui(lambda: status_lbl.configure(
                        text="Cookies indisponibles — analyse sans cookies (4K limitée)…",
                        text_color=WARN_ORANGE))
                try:
                    opts = dict(info_opts)
                    opts.update(attempt)
                    with yt_dlp.YoutubeDL(opts) as ydl:
                        info = ydl.extract_info(url, download=False)
                    cookie_opts = attempt   # on retient la source qui a marche (pour le download)
                    break
                except Exception as e:
                    last_err = e
            if info is None:
                raise last_err or Exception("Analyse impossible")

            video_title = info.get('title', 'Vidéo YouTube')
            video_id = info.get('id', 'temp')
            total_duration = info.get('duration') or 0
            channel = info.get('channel') or info.get('uploader') or ''

            if self.active_tasks[task_id]['cancel_requested']:
                raise Exception("Annulé par l'utilisateur")

            display_title = video_title
            if channel:
                display_title += f" - {channel}"   # ex: "Titre - AppleTrack"
            if start_str or end_str:
                display_title += f" (Extrait {start_str or '00:00'} - {end_str or 'Fin'})"
            safe_title = "".join(c for c in display_title if c.isalpha() or c.isdigit() or c in ' .-_()').rstrip()
            if not safe_title:
                safe_title = f"video_{video_id}"
            self.ui(lambda: title_lbl.configure(text=safe_title))

            if subs_only:
                final_ext = 'srt'
            elif audio_mode:
                final_ext = audio_codec
            elif native_video:
                final_ext = None  # déterminé après le téléchargement
            else:
                final_ext = 'mov' if want_prores else 'mp4'
            temp_output = os.path.join(self.download_dir, f"temp_{video_id}.mp4")
            final_output = os.path.join(self.download_dir, f"{safe_title}.{final_ext}") if final_ext else None
            self.active_tasks[task_id]['temp_output'] = temp_output
            self.active_tasks[task_id]['final_output'] = final_output

            def progress_hook(d):
                if self.active_tasks[task_id]['cancel_requested']:
                    raise Exception("Annulé par l'utilisateur")
                if d['status'] == 'downloading':
                    total = d.get('total_bytes') or d.get('total_bytes_estimate') or 0
                    downloaded = d.get('downloaded_bytes', 0)
                    if total > 0:
                        percent = downloaded / total
                        self.ui(lambda p=percent: prog_bar.set(p))
                        self.ui(lambda p=percent: status_lbl.configure(
                            text=f"Téléchargement… {int(p * 100)}%", text_color=MUTED))

            has_range = start_seconds is not None or end_seconds is not None
            if has_range:
                s_val = start_seconds if start_seconds is not None else 0
                e_val = end_seconds if end_seconds is not None else total_duration
                segment_duration = max(e_val - s_val, 1)
            else:
                s_val, e_val, segment_duration = 0, total_duration, total_duration

            if subs_only:
                # --- Sous-titres seuls : aucun media telecharge ---
                self.ui(lambda: status_lbl.configure(text="Téléchargement des sous-titres…", text_color=MUTED))
                so = {
                    'quiet': True, 'noplaylist': True, 'skip_download': True,
                    'outtmpl': os.path.join(self.download_dir, f"{safe_title}.%(ext)s"),
                    'ffmpeg_location': self.ffmpeg_path,
                    'extractor_args': YOUTUBE_EXTRACTOR_ARGS, 'remote_components': dl_remote,
                    'writesubtitles': True, 'writeautomaticsub': True,
                    'subtitleslangs': ['fr', 'en'], 'subtitlesformat': 'srt/best',
                    'postprocessors': [{'key': 'FFmpegSubtitlesConvertor', 'format': 'srt'}],
                }
                so.update(cookie_opts)
                with yt_dlp.YoutubeDL(so) as ydl:
                    ydl.download([url])
                import glob
                pat = glob.escape(os.path.join(self.download_dir, safe_title))
                srts = sorted(glob.glob(pat + '.*.srt')) + sorted(glob.glob(pat + '.srt'))
                if not srts:
                    raise Exception("Aucun sous-titre disponible pour cette vidéo.")
                final_output = srts[0]
                self.active_tasks[task_id]['final_output'] = final_output
            elif audio_mode:
                # --- Audio seul : telechargement COMPLET (natif, compatible SABR) puis decoupe/conversion ---
                self.ui(lambda: status_lbl.configure(text="Téléchargement audio…", text_color=MUTED))
                a_opts = {
                    'format': 'bestaudio/best', 'quiet': True, 'noplaylist': True,
                    'outtmpl': os.path.join(self.download_dir, f"temp_{video_id}.%(ext)s"),
                    'ffmpeg_location': self.ffmpeg_path, 'progress_hooks': [progress_hook],
                    'extractor_args': YOUTUBE_EXTRACTOR_ARGS, 'remote_components': dl_remote,
                }
                a_opts.update(extra_opts)
                if extra_pps:
                    a_opts['postprocessors'] = extra_pps
                a_opts.update(cookie_opts)
                with yt_dlp.YoutubeDL(a_opts) as ydl:
                    ydl.download([url])
                import glob as _g
                cand = [p for p in _g.glob(_g.escape(os.path.join(self.download_dir, f"temp_{video_id}")) + '.*')
                        if os.path.splitext(p)[1].lower() not in ('.srt', '.jpg', '.jpeg', '.png', '.webp', '.part')]
                if not cand:
                    raise Exception("Téléchargement audio échoué.")
                a_src = cand[0]
                self.ui(lambda: status_lbl.configure(text="Conversion audio…", text_color=MUTED))
                acodec = ['-c:a', 'libmp3lame', '-q:a', '2'] if audio_codec == 'mp3' else ['-c:a', 'pcm_s16le']
                seek = (['-ss', str(s_val)] if has_range else [])
                dur = (['-t', str(segment_duration)] if has_range else [])
                cmd = [self.ffmpeg_path, '-y', '-progress', 'pipe:1'] + seek + ['-i', a_src] + dur + ['-vn'] + acodec + [final_output]
                if self.run_ffmpeg_with_progress(cmd, segment_duration, task_id, status_lbl, prog_bar, "Conversion audio…") != 0:
                    raise Exception("La conversion audio a échoué.")
                if os.path.exists(a_src):
                    os.remove(a_src)
            else:
                # --- Video : telechargement COMPLET (natif, compatible SABR) puis decoupe LOCALE du segment.
                #     On n'utilise PLUS download_ranges (ffmpeg fetch la plage) : ca bloque indefiniment
                #     sur les sessions SABR (formats sans URL directe). cf yt-dlp #12482. ---
                self.ui(lambda: status_lbl.configure(
                    text=("Téléchargement (segment découpé ensuite)…" if has_range else "Téléchargement…"),
                    text_color=MUTED))

                temp_base = os.path.join(self.download_dir, f"temp_{video_id}")
                if native_video:
                    # Pas de merge_output_format -> format natif yt-dlp (.webm, .mkv, .mp4...)
                    dl_outtmpl = temp_base + ".%(ext)s"
                    ydl_opts = {
                        'format': dl_format, 'format_sort': FORMAT_SORT,
                        'outtmpl': dl_outtmpl, 'ffmpeg_location': self.ffmpeg_path,
                        'progress_hooks': [progress_hook], 'quiet': True, 'noplaylist': True,
                        'extractor_args': YOUTUBE_EXTRACTOR_ARGS, 'remote_components': dl_remote,
                    }
                else:
                    ydl_opts = {
                        'format': dl_format, 'format_sort': FORMAT_SORT, 'merge_output_format': 'mp4',
                        'outtmpl': temp_output, 'ffmpeg_location': self.ffmpeg_path,
                        'progress_hooks': [progress_hook], 'quiet': True, 'noplaylist': True,
                        'extractor_args': YOUTUBE_EXTRACTOR_ARGS, 'remote_components': dl_remote,
                    }
                ydl_opts.update(extra_opts)
                if extra_pps:
                    ydl_opts['postprocessors'] = extra_pps
                ydl_opts.update(cookie_opts)

                def run_download(fmt):
                    ydl_opts['format'] = fmt
                    with yt_dlp.YoutubeDL(ydl_opts) as ydl:
                        ydl.download([url])

                def cleanup_partial():
                    if native_video:
                        import glob as _glc
                        for p in _glc.glob(_glc.escape(temp_base) + '.*'):
                            try:
                                os.remove(p)
                            except Exception:
                                pass
                    else:
                        for p in (temp_output, temp_output + ".part"):
                            try:
                                if p and os.path.exists(p):
                                    os.remove(p)
                            except Exception:
                                pass

                # Auto-resilience : si le format demande echoue (4K bloquee/403), repli en <=1080p.
                try:
                    run_download(dl_format)
                except Exception:
                    if self.active_tasks[task_id]['cancel_requested']:
                        raise
                    if dl_format != fallback_format:
                        cleanup_partial()
                        self.ui(lambda: status_lbl.configure(text="Qualité max indisponible — repli en 1080p…",
                                                             text_color=WARN_ORANGE))
                        run_download(fallback_format)
                    else:
                        raise

                if self.active_tasks[task_id]['cancel_requested']:
                    raise Exception("Annulé par l'utilisateur")

                seek = (['-ss', str(s_val)] if has_range else [])
                dur = (['-t', str(segment_duration)] if has_range else [])

                if native_video:
                    # Trouver le fichier téléchargé (extension inconnue à l'avance)
                    import glob as _gln
                    cands = [p for p in _gln.glob(_gln.escape(temp_base) + '.*')
                             if not p.endswith('.part') and
                             os.path.splitext(p)[1].lower() not in ('.srt', '.jpg', '.jpeg', '.png', '.webp')]
                    if not cands:
                        raise Exception("Fichier téléchargé introuvable.")
                    temp_output = cands[0]
                    actual_ext = os.path.splitext(temp_output)[1]
                    final_output = os.path.join(self.download_dir, f"{safe_title}{actual_ext}")
                    self.active_tasks[task_id]['temp_output'] = temp_output
                    self.active_tasks[task_id]['final_output'] = final_output
                    if has_range:
                        # Découpe par copie de flux : rapide, sans ré-encodage (légère imprécision aux points de coupe)
                        self.ui(lambda: status_lbl.configure(text="Découpe du segment (copie flux)…", text_color=MUTED))
                        cmd = ([self.ffmpeg_path, '-y', '-progress', 'pipe:1'] + seek +
                               ['-i', temp_output] + dur + ['-c', 'copy', final_output])
                        if self.run_ffmpeg_with_progress(cmd, segment_duration, task_id, status_lbl, prog_bar, "Découpe…") != 0:
                            raise Exception("La découpe du segment a échoué.")
                        if os.path.exists(temp_output):
                            os.remove(temp_output)
                    else:
                        self.ui(lambda: status_lbl.configure(text="Finalisation…", text_color=MUTED))
                        if os.path.exists(final_output):
                            os.remove(final_output)
                        os.replace(temp_output, final_output)
                else:
                    codec = self._probe_video_codec(temp_output)
                    needs_transcode = want_prores or (codec not in PREMIERE_READY_CODECS)

                    if not needs_transcode and not has_range:
                        # H.264 complet -> deja pret, aucun ré-encodage
                        self.ui(lambda: status_lbl.configure(text="Finalisation…", text_color=MUTED))
                        if os.path.exists(final_output):
                            os.remove(final_output)
                        os.replace(temp_output, final_output)
                    elif not needs_transcode and has_range:
                        # H.264 + segment -> découpe PRECISE (ré-encodage libx264 ; le 'copy' s'aligne sur
                        # une keyframe et déborde de plusieurs secondes -> mauvais pour le montage). On reste
                        # en H.264 (rapide sur un court extrait), pas de HEVC inutile.
                        self.ui(lambda: status_lbl.configure(text="Découpe du segment…", text_color=MUTED))
                        cmd = [self.ffmpeg_path, '-y', '-progress', 'pipe:1'] + seek + ['-i', temp_output] + dur + \
                            ['-c:v', 'libx264', '-crf', '18', '-preset', 'fast', '-pix_fmt', 'yuv420p',
                             '-c:a', 'aac', '-b:a', '256k', '-movflags', '+faststart', final_output]
                        if self.run_ffmpeg_with_progress(cmd, segment_duration, task_id, status_lbl, prog_bar, "Découpe…") != 0:
                            raise Exception("La découpe du segment a échoué.")
                        if os.path.exists(temp_output):
                            os.remove(temp_output)
                    else:
                        is_mac = sys.platform.startswith("darwin")
                        if want_prores:
                            label = "Conversion ProRes…"
                            venc = ['-c:v', 'prores_videotoolbox', '-profile:v', '3'] if is_mac \
                                else ['-c:v', 'prores_ks', '-profile:v', '3']
                            aenc = ['-c:a', 'pcm_s16le']
                            venc_cpu, aenc_cpu = ['-c:v', 'prores_ks', '-profile:v', '3'], ['-c:a', 'pcm_s16le']
                        else:
                            label = "Conversion H.265 (Premiere Pro)…"
                            venc = ['-c:v', 'hevc_videotoolbox', '-q:v', '65', '-tag:v', 'hvc1'] if is_mac \
                                else ['-c:v', 'hevc_nvenc', '-rc', 'vbr', '-cq', '18', '-pix_fmt', 'yuv420p', '-tag:v', 'hvc1']
                            aenc = ['-c:a', 'aac', '-b:a', '256k']
                            venc_cpu = ['-c:v', 'libx265', '-crf', '18', '-preset', 'fast', '-tag:v', 'hvc1']
                            aenc_cpu = ['-c:a', 'aac', '-b:a', '256k']

                        self.ui(lambda l=label: status_lbl.configure(text=l, text_color=MUTED))
                        self.ui(lambda: prog_bar.set(0))
                        cmd = [self.ffmpeg_path, '-y', '-progress', 'pipe:1'] + seek + ['-i', temp_output] + dur + venc + aenc + [final_output]
                        rc = self.run_ffmpeg_with_progress(cmd, segment_duration, task_id, status_lbl, prog_bar, label)

                        if self.active_tasks[task_id]['cancel_requested']:
                            raise Exception("Annulé par l'utilisateur")

                        if rc != 0:
                            self.ui(lambda: status_lbl.configure(text="Encodage CPU (secours)…", text_color=WARN_ORANGE))
                            self.ui(lambda: prog_bar.set(0))
                            cmd2 = [self.ffmpeg_path, '-y', '-progress', 'pipe:1'] + seek + ['-i', temp_output] + dur + venc_cpu + aenc_cpu + [final_output]
                            rc2 = self.run_ffmpeg_with_progress(cmd2, segment_duration, task_id, status_lbl, prog_bar, "Encodage CPU…")
                            if self.active_tasks[task_id]['cancel_requested']:
                                raise Exception("Annulé par l'utilisateur")
                            if rc2 != 0:
                                raise Exception("Le transcodage vidéo a échoué.")

                        if os.path.exists(temp_output):
                            os.remove(temp_output)

            # Sous-titres (.srt) / miniature (.jpg) ont ete ecrits a cote du fichier telecharge
            # (basename du download). Pour la video, ce basename = temp_<id> -> on deplace les
            # annexes vers le nom final. Pour l'audio, elles sont deja au bon nom.
            if (subs or thumb) and not subs_only:
                import glob
                src_base = os.path.join(self.download_dir, f"temp_{video_id}")  # base des annexes ecrites au DL
                dst_base = os.path.splitext(final_output)[0]                     # .../Titre - Chaine
                for p in glob.glob(glob.escape(src_base) + '.*'):
                    low = p.lower()
                    if not (low.endswith('.srt') or low.endswith(('.jpg', '.jpeg', '.png', '.webp'))):
                        continue  # on ne deplace QUE sous-titres et miniature
                    suffix = os.path.basename(p)[len(os.path.basename(src_base)):]  # ex: '.fr.srt', '.jpg'
                    try:
                        os.replace(p, dst_base + suffix)
                    except Exception:
                        pass

            self.ui(lambda: prog_bar.set(1.0))
            if subs_only:
                done_msg = "Terminé ✓ Sous-titres (.srt)"
            elif audio_mode:
                done_msg = f"Terminé ✓ Audio {audio_codec.upper()}"
            elif want_prores:
                done_msg = "Terminé ✓ ProRes (.mov)"
            elif native_video:
                done_msg = f"Terminé ✓ Vidéo native ({os.path.splitext(final_output)[1]})"
            else:
                done_msg = "Terminé ✓ Prêt pour Premiere Pro"
            self.ui(lambda m=done_msg: status_lbl.configure(text=m, text_color=OK_GREEN))
            self.ui(lambda: action_btn.configure(
                text="Ouvrir le dossier", state="normal", fg_color="#27ae60",
                hover_color=OK_GREEN, text_color="#FFFFFF",
                command=lambda: self.open_file_folder(final_output)))

        except Exception as e:
            if self.active_tasks.get(task_id, {}).get('cancel_requested'):
                self.ui(lambda: status_lbl.configure(text="Annulé.", text_color=WARN_ORANGE))
                self.ui(lambda: action_btn.configure(text="Annulé", state="disabled",
                                                     fg_color="#3A3A3A", text_color=MUTED))
            else:
                msg = str(e)
                if self._is_cookie_error(msg):
                    # Echec lie a la connexion/cookies : message clair + bouton "Reparer" -> doc.
                    self.ui(lambda: status_lbl.configure(
                        text="YouTube demande une connexion (vérification anti-robot). "
                             "Tes cookies sont absents ou expirés — clique sur « Réparer ».",
                        text_color=ERR_RED))
                    self.ui(lambda: action_btn.configure(
                        text="Réparer", state="normal",
                        fg_color=ACCENT, hover_color=ACCENT_HOVER, text_color="#FFFFFF",
                        command=self.open_cookie_help))
                else:
                    self.ui(lambda m=msg: status_lbl.configure(text=f"Échec : {m}", text_color=ERR_RED))
                    self.ui(lambda: action_btn.configure(text="Échec", state="disabled",
                                                         fg_color="#3A3A3A", text_color=MUTED))
            self.ui(lambda: prog_bar.set(0))
            try:
                for p in (temp_output, final_output, (temp_output + ".part") if temp_output else None):
                    if p and os.path.exists(p):
                        os.remove(p)
            except Exception:
                pass
        finally:
            self.active_tasks.pop(task_id, None)

    # YouTube exige parfois une connexion (verification anti-robot, video reservee aux membres,
    # restriction d'age) ou bloque l'IP residentielle en 403 : tout ca se debloque avec des cookies.
    # On reconnait ces echecs pour afficher un message clair + le bouton "Reparer".
    _COOKIE_ERR_MARKERS = (
        "sign in to confirm", "not a bot", "confirm your age",
        "age-restricted", "age restricted",
        "only available to members", "members-only", "join this channel",
        "cookies", "cookie", "use --cookies",
        "http error 403", "403: forbidden",
    )

    def _is_cookie_error(self, msg):
        m = (msg or "").lower()
        return any(k in m for k in self._COOKIE_ERR_MARKERS)

    def open_cookie_help(self):
        webbrowser.open(COOKIE_FIX_URL)

    # ---------- Verification de mise a jour ----------
    def _check_update_async(self):
        """En tache de fond au lancement : interroge l'API GitHub Releases (repo public, sans auth)
        et, si le dernier tag est plus recent qu'APP_VERSION, affiche la banniere. Echoue en silence
        (reseau coupe, quota API, pas de release...)."""
        def work():
            try:
                req = urllib.request.Request(RELEASES_API_URL, headers={
                    'User-Agent': 'Robloader', 'Accept': 'application/vnd.github+json'})
                with urllib.request.urlopen(req, timeout=10, context=_ssl_context()) as r:
                    data = json.load(r)
                latest = str(data.get('tag_name', '')).lstrip('vV').strip()
                if not latest or _norm_version(latest) <= _norm_version(APP_VERSION):
                    return
                # URL de l'installeur de CETTE plateforme (sinon on retombera sur la page Releases).
                want = UPDATE_ASSET.get('win' if sys.platform.startswith('win') else sys.platform)
                asset_url = None
                if want:
                    for a in data.get('assets', []):
                        if a.get('name') == want:
                            asset_url = a.get('browser_download_url')
                            break
                self.ui(lambda: self._show_update_banner(latest, asset_url))
            except Exception:
                pass
        threading.Thread(target=work, daemon=True).start()

    def _show_update_banner(self, latest, asset_url):
        self._update_asset_url = asset_url
        self.update_lbl.configure(text=f"Mise à jour disponible : v{latest}  (tu as v{APP_VERSION})")
        self.update_banner.pack(fill="x", padx=24, pady=(4, 0), after=self._header)

    def _start_update(self):
        """Telecharge l'installeur de la plateforme puis le lance. Si pas d'asset pour cette
        plateforme, ouvre simplement la page Releases dans le navigateur."""
        url = getattr(self, '_update_asset_url', None)
        if not url:
            webbrowser.open(RELEASES_PAGE_URL)
            return
        self.update_lbl.configure(text="Téléchargement de la mise à jour…")
        threading.Thread(target=self._download_and_launch, args=(url,), daemon=True).start()

    def _download_and_launch(self, url):
        try:
            name = os.path.basename(url.split('?')[0]) or "Robloader-update"
            dest = os.path.join(self.temp_dir, name)
            req = urllib.request.Request(url, headers={'User-Agent': 'Robloader'})
            with urllib.request.urlopen(req, timeout=30, context=_ssl_context()) as r:
                total = int(r.headers.get('Content-Length') or 0)
                got = 0
                with open(dest, 'wb') as f:
                    while True:
                        chunk = r.read(262144)
                        if not chunk:
                            break
                        f.write(chunk)
                        got += len(chunk)
                        if total:
                            pct = int(got * 100 / total)
                            self.ui(lambda p=pct: self.update_lbl.configure(text=f"Téléchargement… {p}%"))
            self.ui(lambda: self._launch_installer(dest))
        except Exception:
            self.ui(lambda: self.update_lbl.configure(
                text="Échec du téléchargement — ouverture de la page Releases…", text_color=ERR_RED))
            webbrowser.open(RELEASES_PAGE_URL)

    def _launch_installer(self, path):
        if sys.platform.startswith('win'):
            # Lance l'installeur puis quitte l'app : les fichiers doivent etre liberes pour l'ecrasement.
            try:
                os.startfile(path)  # noqa: disponible uniquement sous Windows
            except Exception:
                subprocess.Popen([path])
            self.update_lbl.configure(text="Installation lancée — Robloader va se fermer…")
            self.after(900, self.destroy)
        elif sys.platform == 'darwin':
            subprocess.run(['open', path])  # monte le .dmg
            self.update_lbl.configure(
                text="DMG ouvert — glisse Robloader dans Applications pour terminer.", text_color=OK_GREEN)
        else:
            webbrowser.open(RELEASES_PAGE_URL)

    def open_file_folder(self, path):
        if sys.platform.startswith("win"):
            subprocess.run(["explorer", "/select,", os.path.normpath(path)])
        elif sys.platform.startswith("darwin"):
            subprocess.run(["open", "-R", path])
        else:
            subprocess.run(["xdg-open", os.path.dirname(path)])


if __name__ == "__main__":
    app = RobloaderApp()
    app.mainloop()
