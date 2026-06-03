import os
import sys
import json
import shutil
import tempfile
import threading
import subprocess
import tkinter as tk
import customtkinter as ctk
from tkinter import filedialog
import yt_dlp

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


def format_for_height(h):
    """Selecteur de format yt-dlp pour une hauteur max donnee (None = sans plafond)."""
    if not h:
        return 'bv*+ba/b'
    return f'bv*[height<={h}]+ba/bv*[height<={h}]/b[height<={h}]/b'


def config_dir():
    if sys.platform.startswith('win'):
        base = os.environ.get('APPDATA') or os.path.expanduser('~')
    elif sys.platform == 'darwin':
        base = os.path.join(os.path.expanduser('~'), 'Library', 'Application Support')
    else:
        base = os.environ.get('XDG_CONFIG_HOME') or os.path.join(os.path.expanduser('~'), '.config')
    return os.path.join(base, 'Robloader')


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


def detect_browser_cookies():
    """Retourne le 1er navigateur dont la base de cookies existe (pour 'cookiesfrombrowser').
    Permet la 4K sans cookies.txt si l'utilisateur est connecte a YouTube dans son navigateur."""
    home = os.path.expanduser("~")
    if sys.platform == 'darwin':
        cand = [
            ('chrome', "Library/Application Support/Google/Chrome"),
            ('brave',  "Library/Application Support/BraveSoftware/Brave-Browser"),
            ('edge',   "Library/Application Support/Microsoft Edge"),
            ('firefox', "Library/Application Support/Firefox/Profiles"),
            ('safari', "Library/Containers/com.apple.Safari/Data/Library/Cookies/Cookies.binarycookies"),
        ]
    elif sys.platform.startswith('win'):
        la = os.environ.get('LOCALAPPDATA', '')
        ap = os.environ.get('APPDATA', '')
        cand = [
            ('chrome', os.path.join(la, "Google", "Chrome", "User Data")),
            ('edge',   os.path.join(la, "Microsoft", "Edge", "User Data")),
            ('brave',  os.path.join(la, "BraveSoftware", "Brave-Browser", "User Data")),
            ('firefox', os.path.join(ap, "Mozilla", "Firefox", "Profiles")),
        ]
    else:
        cand = [
            ('chrome', os.path.join(home, ".config", "google-chrome")),
            ('firefox', os.path.join(home, ".mozilla", "firefox")),
        ]
    for name, rel in cand:
        p = rel if os.path.isabs(rel) else os.path.join(home, rel)
        if os.path.exists(p):
            return name
    return None


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
        self.browser_cookies = detect_browser_cookies()

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
        elif self.browser_cookies:
            bits.append(f"cookies {self.browser_cookies} ✓")
        else:
            bits.append("sans cookies (4K limitée)")
        if not self.js_runtime:
            bits.append("Deno absent → 1080p max")
        self.path_label.configure(text="     ".join(bits))

    def _on_quality_change(self, value):
        self.config_data["quality"] = value
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
        url = self.url_entry.get().strip()
        if not url:
            return

        start_val = self.start_entry.get().strip()
        end_val = self.end_entry.get().strip()
        quality_label = self.quality_var.get()

        self.url_entry.delete(0, "end")
        self.start_entry.delete(0, "end")
        self.end_entry.delete(0, "end")

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
            args=(url, start_val, end_val, quality_label, task_id,
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

    def _cookie_opts(self):
        if os.path.exists(self.cookie_path):
            return {'cookiefile': self.cookie_path}
        if self.browser_cookies:
            return {'cookiesfrombrowser': (self.browser_cookies,)}
        return {}

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

    def download_pipeline(self, url, start_str, end_str, quality_label, task_id,
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
            cookie_opts = self._cookie_opts()

            self.ui(lambda: status_lbl.configure(text="Analyse de la vidéo…", text_color=MUTED))

            info_opts = {
                'quiet': True, 'extractor_args': YOUTUBE_EXTRACTOR_ARGS,
                'remote_components': dl_remote, 'format': dl_format, 'format_sort': FORMAT_SORT,
            }

            def _do_extract():
                opts = dict(info_opts)
                opts.update(cookie_opts)
                with yt_dlp.YoutubeDL(opts) as ydl:
                    return ydl.extract_info(url, download=False)

            # Robustesse cookies : si la lecture des cookies NAVIGATEUR echoue (keychain refuse,
            # Safari sans Full Disk Access, navigateur verrouille), on reessaie SANS cookies au lieu
            # de planter. Un cookies.txt (fichier), lui, n'est pas concerne par ce repli.
            try:
                info = _do_extract()
            except Exception:
                if self.active_tasks[task_id]['cancel_requested']:
                    raise
                if 'cookiesfrombrowser' in cookie_opts:
                    cookie_opts = {}
                    self.ui(lambda: status_lbl.configure(
                        text="Cookies navigateur indisponibles — analyse sans cookies…",
                        text_color=WARN_ORANGE))
                    info = _do_extract()
                else:
                    raise
            video_title = info.get('title', 'Vidéo YouTube')
            video_id = info.get('id', 'temp')
            total_duration = info.get('duration') or 0

            if self.active_tasks[task_id]['cancel_requested']:
                raise Exception("Annulé par l'utilisateur")

            display_title = video_title
            if start_str or end_str:
                display_title += f" (Extrait {start_str or '00:00'} - {end_str or 'Fin'})"
            safe_title = "".join(c for c in display_title if c.isalpha() or c.isdigit() or c in ' .-_()').rstrip()
            if not safe_title:
                safe_title = f"video_{video_id}"
            self.ui(lambda: title_lbl.configure(text=safe_title))

            temp_output = os.path.join(self.download_dir, f"temp_{video_id}.mp4")
            final_output = os.path.join(self.download_dir, f"{safe_title}.mp4")
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

            ydl_opts = {
                'format': dl_format, 'format_sort': FORMAT_SORT, 'merge_output_format': 'mp4',
                'outtmpl': temp_output, 'ffmpeg_location': self.ffmpeg_path,
                'progress_hooks': [progress_hook], 'quiet': True,
                'extractor_args': YOUTUBE_EXTRACTOR_ARGS, 'remote_components': dl_remote,
            }
            ydl_opts.update(cookie_opts)

            if start_seconds is not None or end_seconds is not None:
                s_val = start_seconds if start_seconds is not None else 0
                e_val = end_seconds if end_seconds is not None else total_duration
                ydl_opts['download_ranges'] = lambda info_dict, yt: [{'start_time': s_val, 'end_time': e_val}]
                ydl_opts['force_keyframes_at_cuts'] = False
                self.ui(lambda: status_lbl.configure(text="Extraction du segment…", text_color=MUTED))
                segment_duration = max(e_val - s_val, 1)
            else:
                segment_duration = total_duration

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

            # --- Transcodage H.265 UNIQUEMENT si necessaire (source VP9/AV1). Le H.264/HEVC en MP4
            #     est deja parfait pour Premiere -> on garde tel quel (gain de temps enorme). ---
            codec = self._probe_video_codec(temp_output)
            needs_transcode = codec not in PREMIERE_READY_CODECS  # vp9/av1/inconnu -> on transcode

            if not needs_transcode:
                self.ui(lambda: status_lbl.configure(text="Finalisation…", text_color=MUTED))
                if os.path.exists(final_output):
                    os.remove(final_output)
                os.replace(temp_output, final_output)
            else:
                self.ui(lambda: status_lbl.configure(text="Conversion H.265 (Premiere Pro)…", text_color=MUTED))
                self.ui(lambda: prog_bar.set(0))

                if sys.platform.startswith("darwin"):
                    encoder_args = ['-c:v', 'hevc_videotoolbox', '-q:v', '65', '-tag:v', 'hvc1']
                else:
                    encoder_args = ['-c:v', 'hevc_nvenc', '-rc', 'vbr', '-cq', '18', '-pix_fmt', 'yuv420p', '-tag:v', 'hvc1']

                ffmpeg_cmd = [self.ffmpeg_path, '-y', '-progress', 'pipe:1', '-i', temp_output] \
                    + encoder_args + ['-c:a', 'aac', '-b:a', '256k', final_output]
                rc = self.run_ffmpeg_with_progress(ffmpeg_cmd, segment_duration, task_id, status_lbl, prog_bar,
                                                   "Conversion GPU…")

                if self.active_tasks[task_id]['cancel_requested']:
                    raise Exception("Annulé par l'utilisateur")

                if rc != 0:
                    self.ui(lambda: status_lbl.configure(text="GPU indisponible — encodage CPU…", text_color=WARN_ORANGE))
                    self.ui(lambda: prog_bar.set(0))
                    cpu_args = ['-c:v', 'libx265', '-crf', '18', '-preset', 'fast', '-tag:v', 'hvc1']
                    ffmpeg_cmd_cpu = [self.ffmpeg_path, '-y', '-progress', 'pipe:1', '-i', temp_output] \
                        + cpu_args + ['-c:a', 'aac', '-b:a', '256k', final_output]
                    rc_cpu = self.run_ffmpeg_with_progress(ffmpeg_cmd_cpu, segment_duration, task_id, status_lbl,
                                                           prog_bar, "Conversion CPU…")
                    if self.active_tasks[task_id]['cancel_requested']:
                        raise Exception("Annulé par l'utilisateur")
                    if rc_cpu != 0:
                        raise Exception("Le transcodage vidéo a échoué.")

                if os.path.exists(temp_output):
                    os.remove(temp_output)

            self.ui(lambda: prog_bar.set(1.0))
            done_msg = "Terminé ✓ Prêt pour Premiere Pro" if needs_transcode \
                else "Terminé ✓ (H.264, sans ré-encodage)"
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
