import os
import sys
import json
import shutil
import tempfile
import threading
import subprocess
import urllib.request


def config_dir():
    if sys.platform.startswith('win'):
        base = os.environ.get('APPDATA') or os.path.expanduser('~')
    elif sys.platform == 'darwin':
        base = os.path.join(os.path.expanduser('~'), 'Library', 'Application Support')
    else:
        base = os.environ.get('XDG_CONFIG_HOME') or os.path.join(os.path.expanduser('~'), '.config')
    return os.path.join(base, 'Robloader')


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
            with urllib.request.urlopen('https://pypi.org/pypi/yt-dlp/json', timeout=12) as r:
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
# web_embedded delivre alors la 4K sans 403. 'missing_pot' = ne pas jeter les formats sans PoT.
YOUTUBE_EXTRACTOR_ARGS = {
    'youtube': {
        'player_client': ['web_embedded', 'tv', 'ios'],
        'formats': ['missing_pot'],
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
OUT_HEVC = "HEVC (Premiere)"
OUT_PRORES = "ProRes (montage)"
OUT_MP3 = "Audio MP3"
OUT_WAV = "Audio WAV"
OUT_SUBS = "Sous-titres seuls (.srt)"
OUTPUT_FORMATS = [OUT_HEVC, OUT_PRORES, OUT_MP3, OUT_WAV, OUT_SUBS]
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

        # Dossier de destination : dernier utilise (memorise) sinon ~/Downloads.
        saved_dir = self.config_data.get("download_dir")
        self.download_dir = saved_dir if (saved_dir and os.path.isdir(saved_dir)) \
            else os.path.join(os.path.expanduser("~"), "Downloads")

        self.temp_dir = pick_writable_tempdir(self.download_dir)
        tempfile.tempdir = self.temp_dir
        os.environ["TEMP"] = self.temp_dir
        os.environ["TMP"] = self.temp_dir

        self._set_window_icon()
        self._build_ui()

        # Met yt-dlp a jour en arriere-plan (effectif au prochain lancement).
        update_ytdlp_async()

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
        ctk.CTkLabel(header, text="Robloader", font=("Helvetica", 24, "bold")).pack(side="left")
        ctk.CTkLabel(header, text="  YouTube → fichier prêt pour Premiere Pro",
                     font=("Helvetica", 12), text_color=MUTED).pack(side="left", pady=(8, 0))

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

        # Ligne 3 : format de sortie + sous-titres + miniature
        row3 = ctk.CTkFrame(self, fg_color="transparent")
        row3.pack(fill="x", padx=24, pady=(4, 2))
        ctk.CTkLabel(row3, text="Sortie  ", font=("Helvetica", 12, "bold")).pack(side="left")
        out_init = self.config_data.get("output", DEFAULT_OUTPUT)
        self.output_var = ctk.StringVar(value=out_init if out_init in OUTPUT_FORMATS else DEFAULT_OUTPUT)
        self.output_menu = ctk.CTkOptionMenu(
            row3, values=OUTPUT_FORMATS, variable=self.output_var, width=170, height=32,
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

        self.url_entry.delete(0, "end")
        self.start_entry.delete(0, "end")
        self.end_entry.delete(0, "end")

        # Batch : on accepte plusieurs URLs separees par des espaces / retours a la ligne.
        for url in [u for u in raw.split() if u.startswith("http")] or [raw]:
            self._enqueue(url, start_val, end_val, quality_label, output, subs, thumb)

    def _enqueue(self, url, start_val, end_val, quality_label, output, subs, thumb):
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
            args=(url, start_val, end_val, quality_label, output, subs, thumb, task_id,
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

    def download_pipeline(self, url, start_str, end_str, quality_label, output, subs, thumb, task_id,
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
            else:
                final_ext = 'mov' if want_prores else 'mp4'
            temp_output = os.path.join(self.download_dir, f"temp_{video_id}.mp4")
            final_output = os.path.join(self.download_dir, f"{safe_title}.{final_ext}")
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

            def _ranges(info_dict, yt):
                return [{'start_time': s_val, 'end_time': e_val}]

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
                # --- Audio seul (MP3/WAV) : aucun transcodage video ---
                self.ui(lambda: status_lbl.configure(text="Téléchargement audio…", text_color=MUTED))
                a_opts = {
                    'format': 'bestaudio/best', 'quiet': True, 'noplaylist': True,
                    'outtmpl': os.path.join(self.download_dir, f"{safe_title}.%(ext)s"),
                    'ffmpeg_location': self.ffmpeg_path, 'progress_hooks': [progress_hook],
                    'extractor_args': YOUTUBE_EXTRACTOR_ARGS, 'remote_components': dl_remote,
                    'postprocessors': [{'key': 'FFmpegExtractAudio',
                                        'preferredcodec': audio_codec, 'preferredquality': '192'}],
                }
                a_opts.update(extra_opts)
                a_opts['postprocessors'] = a_opts['postprocessors'] + extra_pps
                a_opts.update(cookie_opts)
                if has_range:
                    a_opts['download_ranges'] = _ranges
                    a_opts['force_keyframes_at_cuts'] = False
                with yt_dlp.YoutubeDL(a_opts) as ydl:
                    ydl.download([url])
                # final_output = safe_title.<codec>, ecrit par le post-processeur audio
            else:
                # --- Video ---
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
                if has_range:
                    ydl_opts['download_ranges'] = _ranges
                    ydl_opts['force_keyframes_at_cuts'] = False
                    self.ui(lambda: status_lbl.configure(text="Extraction du segment…", text_color=MUTED))

                def run_download(fmt):
                    ydl_opts['format'] = fmt
                    with yt_dlp.YoutubeDL(ydl_opts) as ydl:
                        ydl.download([url])

                def cleanup_partial():
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

                # Transcodage : ProRes (toujours) ou H.265 (seulement si source VP9/AV1 ; le H.264
                # est deja parfait pour Premiere -> garde tel quel, gain de temps enorme).
                codec = self._probe_video_codec(temp_output)
                needs_transcode = want_prores or (codec not in PREMIERE_READY_CODECS)

                if not needs_transcode:
                    self.ui(lambda: status_lbl.configure(text="Finalisation…", text_color=MUTED))
                    if os.path.exists(final_output):
                        os.remove(final_output)
                    os.replace(temp_output, final_output)
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
                    cmd = [self.ffmpeg_path, '-y', '-progress', 'pipe:1', '-i', temp_output] + venc + aenc + [final_output]
                    rc = self.run_ffmpeg_with_progress(cmd, segment_duration, task_id, status_lbl, prog_bar, label)

                    if self.active_tasks[task_id]['cancel_requested']:
                        raise Exception("Annulé par l'utilisateur")

                    if rc != 0:
                        self.ui(lambda: status_lbl.configure(text="Encodage CPU (secours)…", text_color=WARN_ORANGE))
                        self.ui(lambda: prog_bar.set(0))
                        cmd2 = [self.ffmpeg_path, '-y', '-progress', 'pipe:1', '-i', temp_output] + venc_cpu + aenc_cpu + [final_output]
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
            if (subs or thumb) and not audio_mode and not subs_only:
                import glob
                src_base = os.path.splitext(temp_output)[0]   # .../temp_<id>
                dst_base = os.path.splitext(final_output)[0]   # .../Titre - Chaine
                for p in glob.glob(glob.escape(src_base) + '.*'):
                    suffix = os.path.basename(p)[len(os.path.basename(src_base)):]  # ex: '.fr.srt', '.jpg'
                    if suffix.lower() in ('.mp4', '.part', '.mov', '.webm', '.mkv'):
                        continue
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
