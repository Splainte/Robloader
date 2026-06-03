import os
import sys
import shutil
import threading
import subprocess
import customtkinter as ctk
from tkinter import filedialog
import yt_dlp

# --- Strategie d'extraction YouTube (2025) ---
# On agrege les formats de plusieurs clients pour obtenir le MAX de qualite (jusqu'a la 4K) :
#   - default          : le jeu de clients par defaut de yt-dlp (le plus a jour).
#   - -tv              : on RETIRE le client TV qui renvoie des formats DRM non telechargeables.
#   - web_safari       : ajoute les formats web HD/4K (necessite la resolution du nsig).
#   - ios              : filet de secours si tout le reste echoue.
# Combiner ces clients evite a la fois le 360p (client mobile seul) et les blocages DRM (client TV).
# 'missing_pot' demande a yt-dlp de NE PAS jeter les formats qui n'ont pas de PO Token
# (sinon, sans fournisseur de PoT, on retombe sur des formats bas debit).
#
# IMPORTANT (anti-blocage "Preparation") : installer Deno (https://deno.com) accelere enormement
# le calcul du nsig sur les longues videos. Voir le README.
YOUTUBE_EXTRACTOR_ARGS = {
    'youtube': {
        'player_client': ['default', '-tv', 'web_safari', 'ios'],
        'formats': ['missing_pot'],
    }
}

# --- Resolution du nsig (INDISPENSABLE pour la 4K / 1440p) ---
# yt-dlp (2025+) ne resout plus le "n challenge" en Python pur : il execute le vrai script JS
# de YouTube via un moteur externe (Deno) + un script solveur "EJS".
#   - Deno doit etre present (installe, ou place a cote de l'exe comme ffmpeg -> detecte via le PATH).
#   - 'ejs:github' telecharge UNE fois (puis met en cache) le script solveur depuis GitHub.
# Sans ca : la 1080p (souvent AVC, sans nsig) passe encore, mais la 4K/1440p (VP9/AV1, web) est
# soit absente soit throttlee/403 -> "ERROR: ffmpeg exited with code ..." sur les IP residentielles.
EJS_REMOTE_COMPONENTS = ['ejs:github']

# bv*+ba/b = meilleure video + meilleur audio, sans plafond de resolution -> 1080p/4K si dispo.
BEST_FORMAT = 'bv*+ba/b'
# Repli quand aucun moteur JS (Deno) n'est dispo : on plafonne a 1080p, format qui ne depend pas
# du nsig -> evite le "ffmpeg exited with code ..." sur la 4K throttlee, au lieu de planter.
FALLBACK_FORMAT = 'bv*[height<=1080]+ba/b[height<=1080]/b'
FORMAT_SORT = ['res', 'fps', 'br']


def has_js_runtime():
    """Vrai si un moteur JS (Deno) est trouvable -> requis par yt-dlp pour resoudre le nsig (4K).
    shutil.which voit aussi un deno bundle a cote de l'exe, car son dossier est injecte dans le PATH."""
    return shutil.which('deno') is not None


class RobloaderApp(ctk.CTk):
    def __init__(self):
        super().__init__()

        self.title("Robloader - Youtube vers h265")
        self.geometry("750x550")
        self.resizable(False, False)

        # Suivi des taches pour la gestion de l'annulation
        self.task_counter = 0
        self.active_tasks = {}

        # --- CONFIGURATION DU CHEMIN DE BASE ---
        if getattr(sys, 'frozen', False):
            self.ffmpeg_base_dir = sys._MEIPASS
            self.app_dir = os.path.dirname(sys.executable)
        else:
            self.ffmpeg_base_dir = os.path.dirname(os.path.abspath(__file__))
            self.app_dir = self.ffmpeg_base_dir

        ffmpeg_filename = "ffmpeg.exe" if sys.platform.startswith("win") else "ffmpeg"
        self.ffmpeg_path = os.path.join(self.ffmpeg_base_dir, ffmpeg_filename)

        # Injection du dossier de FFmpeg dans le PATH systeme
        # (sert aussi a detecter un deno.exe bundle a cote de l'app)
        os.environ["PATH"] += os.pathsep + self.ffmpeg_base_dir

        # Deno present ? -> conditionne la 4K (resolution du nsig). Sinon on plafonne a 1080p.
        self.js_runtime = has_js_runtime()

        self.cookie_path = os.path.join(self.app_dir, "cookies.txt")
        self.download_dir = os.path.join(os.path.expanduser("~"), "Downloads")

        # --- ZONE SUPÉRIEURE (COMMANDES ROW 1) ---
        self.top_frame = ctk.CTkFrame(self, fg_color="transparent")
        self.top_frame.pack(pady=(20, 5), padx=20, fill="x")

        self.url_entry = ctk.CTkEntry(self.top_frame, placeholder_text="Collez le lien YouTube ici...", width=400)
        self.url_entry.pack(side="left", padx=5)

        self.folder_btn = ctk.CTkButton(self.top_frame, text="Destination", width=110, fg_color="#4A4A4A", hover_color="#5A5A5A", command=self.choose_folder)
        self.folder_btn.pack(side="left", padx=5)

        self.download_btn = ctk.CTkButton(self.top_frame, text="Telecharger", width=120, fg_color="#1f538d", font=("Arial", 12, "bold"), command=self.start_download_thread)
        self.download_btn.pack(side="left", padx=5)

        # --- ZONE TIMECODE (ROW 2) ---
        self.time_frame = ctk.CTkFrame(self, fg_color="transparent")
        self.time_frame.pack(pady=(0, 5), padx=20, fill="x")

        self.start_lbl = ctk.CTkLabel(self.time_frame, text="Debut :", font=("Arial", 11, "bold"))
        self.start_lbl.pack(side="left", padx=(5, 2))
        self.start_entry = ctk.CTkEntry(self.time_frame, placeholder_text="00:00", width=70)
        self.start_entry.pack(side="left", padx=(0, 15))

        self.end_lbl = ctk.CTkLabel(self.time_frame, text="Fin :", font=("Arial", 11, "bold"))
        self.end_lbl.pack(side="left", padx=(5, 2))
        self.end_entry = ctk.CTkEntry(self.time_frame, placeholder_text="Ex: 01:30", width=70)
        self.end_entry.pack(side="left", padx=(0, 15))

        self.time_hint_lbl = ctk.CTkLabel(self.time_frame, text="(Optionnel. Format MM:SS ou HH:MM:SS)", font=("Arial", 10), text_color="gray")
        self.time_hint_lbl.pack(side="left", padx=5)

        self.path_label = ctk.CTkLabel(self, text=f"Enregistrement dans : {self.download_dir}", font=("Arial", 10), text_color="gray")
        self.path_label.pack(anchor="w", padx=25, pady=(0, 10))

        # Avertit si Deno manque : la 4K sera indisponible, on reste en 1080p (mais ca marche).
        if not self.js_runtime:
            self.warn_label = ctk.CTkLabel(
                self,
                text="⚠ Deno introuvable : qualité plafonnée à 1080p. Placez deno(.exe) à côté de l'app pour activer la 4K.",
                font=("Arial", 10), text_color="#e67e22"
            )
            self.warn_label.pack(anchor="w", padx=25, pady=(0, 8))

        # --- ZONE INFÉRIEURE (LISTE DÉFILANTE) ---
        self.scroll_frame = ctk.CTkScrollableFrame(self, label_text="File d'attente des téléchargements")
        self.scroll_frame.pack(padx=20, pady=10, fill="both", expand=True)

    # --- Pont thread -> interface ---
    # Tkinter/customtkinter N'EST PAS thread-safe : modifier un widget depuis un thread
    # secondaire peut figer ou crasher l'UI (symptome typique : reste bloque sur "Preparation").
    # Toute mise a jour d'interface lancee depuis download_pipeline / ffmpeg passe donc par ici.
    def ui(self, fn):
        try:
            self.after(0, fn)
        except Exception:
            pass

    def choose_folder(self):
        folder = filedialog.askdirectory(initialdir=self.download_dir)
        if folder:
            self.download_dir = folder
            self.path_label.configure(text=f"Enregistrement dans : {self.download_dir}")

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

        self.url_entry.delete(0, "end")
        self.start_entry.delete(0, "end")
        self.end_entry.delete(0, "end")

        self.task_counter += 1
        task_id = self.task_counter

        task_frame = ctk.CTkFrame(self.scroll_frame, fg_color="#2B2B2B")
        task_frame.pack(fill="x", pady=5, padx=5)

        title_label = ctk.CTkLabel(task_frame, text="Analyse du lien...", font=("Arial", 12, "bold"), anchor="w")
        title_label.pack(fill="x", padx=10, pady=(5, 0))

        status_label = ctk.CTkLabel(task_frame, text="Preparation...", font=("Arial", 10), text_color="#A0A0A0", anchor="w")
        status_label.pack(fill="x", padx=10)

        progress_bar = ctk.CTkProgressBar(task_frame)
        progress_bar.pack(fill="x", padx=10, pady=5)
        progress_bar.set(0)

        action_btn = ctk.CTkButton(
            task_frame,
            text="Annuler",
            width=120,
            fg_color="#FFFFFF",
            text_color="#000000",
            hover_color="#E0E0E0",
            command=lambda: self.cancel_task(task_id)
        )
        action_btn.pack(side="right", padx=10, pady=5)

        self.active_tasks[task_id] = {
            'cancel_requested': False,
            'process': None,
            'action_btn': action_btn,
            'status_label': status_label,
            'progress_bar': progress_bar,
            'temp_output': None,
            'final_output': None
        }

        thread = threading.Thread(target=self.download_pipeline, args=(url, start_val, end_val, task_id, title_label, status_label, progress_bar, action_btn))
        thread.start()

    def cancel_task(self, task_id):
        if task_id in self.active_tasks:
            self.active_tasks[task_id]['cancel_requested'] = True
            self.active_tasks[task_id]['status_label'].configure(text="Annulation en cours...", text_color="#e67e22")
            process = self.active_tasks[task_id]['process']
            if process:
                try:
                    process.terminate()
                except Exception:
                    pass

    def run_ffmpeg_with_progress(self, cmd, duration, task_id, status_lbl, prog_bar, step_text):
        startupinfo = None
        if sys.platform.startswith("win"):
            startupinfo = subprocess.STARTUPINFO()
            startupinfo.dwFlags |= subprocess.STARTF_USESHOWWINDOW

        process = subprocess.Popen(
            cmd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
            universal_newlines=True, encoding='utf-8', startupinfo=startupinfo
        )

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
                        percent = time_us / (duration * 1000000)
                        percent = min(max(percent, 0.0), 1.0)
                        # On ne rafraichit l'UI que par paliers de 1% pour ne pas saturer
                        # la boucle Tkinter (ffmpeg emet beaucoup de lignes par seconde).
                        if int(percent * 100) != last_percent:
                            last_percent = int(percent * 100)
                            self.ui(lambda p=percent: prog_bar.set(p))
                            self.ui(lambda p=percent: status_lbl.configure(text=f"{step_text} {int(p * 100)}%"))
                except Exception:
                    pass

        process.wait()
        return process.returncode

    def download_pipeline(self, url, start_str, end_str, task_id, title_lbl, status_lbl, prog_bar, action_btn):
        temp_output = None
        final_output = None
        try:
            start_seconds = self.parse_timecode(start_str)
            end_seconds = self.parse_timecode(end_str)

            # Deno present -> 4K (nsig resolu via EJS). Sinon -> repli 1080p sans nsig.
            dl_format = BEST_FORMAT if self.js_runtime else FALLBACK_FORMAT
            dl_remote = EJS_REMOTE_COMPONENTS if self.js_runtime else []

            self.ui(lambda: status_lbl.configure(text="Analyse de la video..."))

            # --- ETAPE 0 : recuperation des metadonnees (titre, duree) ---
            ydl_info_opts = {
                'quiet': True,
                'extractor_args': YOUTUBE_EXTRACTOR_ARGS,
                'remote_components': dl_remote,
                'format': dl_format,
                'format_sort': FORMAT_SORT,
            }
            if os.path.exists(self.cookie_path):
                ydl_info_opts['cookiefile'] = self.cookie_path

            with yt_dlp.YoutubeDL(ydl_info_opts) as ydl:
                info = ydl.extract_info(url, download=False)
                video_title = info.get('title', 'Video YouTube')
                video_id = info.get('id', 'temp')
                total_duration = info.get('duration') or 0

            if self.active_tasks[task_id]['cancel_requested']:
                raise Exception("Annulé par l'utilisateur")

            display_title = video_title
            if start_str or end_str:
                display_title += f" (Extrait {start_str if start_str else '00:00'} - {end_str if end_str else 'Fin'})"

            safe_title = "".join([c for c in display_title if c.isalpha() or c.isdigit() or c in ' .-_()']).rstrip()
            self.ui(lambda: title_lbl.configure(text=safe_title))

            temp_output = os.path.join(self.download_dir, f"temp_{video_id}.mp4")
            final_output = os.path.join(self.download_dir, f"{safe_title}.mp4")

            self.active_tasks[task_id]['temp_output'] = temp_output
            self.active_tasks[task_id]['final_output'] = final_output

            def progress_hook(d):
                # Lance par yt-dlp dans CE thread -> on ne touche pas l'UI directement, on passe par self.ui
                if self.active_tasks[task_id]['cancel_requested']:
                    raise Exception("Annulé par l'utilisateur")
                if d['status'] == 'downloading':
                    total = d.get('total_bytes') or d.get('total_bytes_estimate') or 0
                    downloaded = d.get('downloaded_bytes', 0)
                    if total > 0:
                        percent = downloaded / total
                        self.ui(lambda p=percent: prog_bar.set(p))
                        self.ui(lambda p=percent: status_lbl.configure(text=f"Etape 1/2 : Telechargement... {int(p*100)}%"))

            ydl_opts = {
                'format': dl_format,
                'format_sort': FORMAT_SORT,
                'merge_output_format': 'mp4',
                'outtmpl': temp_output,
                'ffmpeg_location': self.ffmpeg_path,
                'progress_hooks': [progress_hook],
                'quiet': True,
                'extractor_args': YOUTUBE_EXTRACTOR_ARGS,
                'remote_components': dl_remote,
            }
            if os.path.exists(self.cookie_path):
                ydl_opts['cookiefile'] = self.cookie_path

            if start_seconds is not None or end_seconds is not None:
                s_val = start_seconds if start_seconds is not None else 0
                e_val = end_seconds if end_seconds is not None else total_duration
                ydl_opts['download_ranges'] = lambda info_dict, yt_instance: [
                    {
                        'start_time': s_val,
                        'end_time': e_val
                    }
                ]
                # Decoupe rapide alignee sur les keyframes (pas de re-encodage au telechargement).
                # La precision finale est de toute facon assuree par le re-encodage HEVC qui suit.
                ydl_opts['force_keyframes_at_cuts'] = False

                # La decoupe par section passe par ffmpeg et peut rester silencieuse un moment :
                # on previent l'utilisateur pour que ca n'ait pas l'air "bloque".
                self.ui(lambda: status_lbl.configure(text="Etape 1/2 : Extraction du segment..."))
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

            # Auto-resilience : si le format MAX (4K) echoue (nsig non resolu -> flux throttle/403
            # -> "ffmpeg exited with code ..."), on retombe proprement en 1080p au lieu de planter.
            try:
                run_download(dl_format)
            except Exception as dl_err:
                if self.active_tasks[task_id]['cancel_requested']:
                    raise
                if dl_format != FALLBACK_FORMAT:
                    cleanup_partial()
                    self.ui(lambda: status_lbl.configure(
                        text="4K indisponible (nsig/Deno) — repli en 1080p...", text_color="#e67e22"))
                    run_download(FALLBACK_FORMAT)
                else:
                    raise

            if self.active_tasks[task_id]['cancel_requested']:
                raise Exception("Annulé par l'utilisateur")

            self.ui(lambda: status_lbl.configure(text="Etape 2/2 : Conversion Premiere Pro (HEVC)..."))
            self.ui(lambda: prog_bar.set(0))

            if sys.platform.startswith("darwin"):
                encoder_args = ['-c:v', 'hevc_videotoolbox', '-q:v', '65']
            else:
                encoder_args = ['-c:v', 'hevc_nvenc', '-rc', 'vbr', '-cq', '18', '-pix_fmt', 'yuv420p']

            ffmpeg_cmd = [
                self.ffmpeg_path, '-y', '-progress', 'pipe:1', '-i', temp_output
            ] + encoder_args + ['-c:a', 'aac', '-b:a', '256k', final_output]

            return_code = self.run_ffmpeg_with_progress(
                ffmpeg_cmd, segment_duration, task_id, status_lbl, prog_bar,
                "Etape 2/2 : Conversion GPU (Optimisee)..."
            )

            if self.active_tasks[task_id]['cancel_requested']:
                raise Exception("Annulé par l'utilisateur")

            if return_code != 0:
                self.ui(lambda: status_lbl.configure(text="GPU non disponible. Bascule sur l'encodage CPU..."))
                self.ui(lambda: prog_bar.set(0))

                cpu_encoder_args = ['-c:v', 'libx265', '-crf', '18', '-preset', 'fast']
                ffmpeg_cmd_cpu = [
                    self.ffmpeg_path, '-y', '-progress', 'pipe:1', '-i', temp_output
                ] + cpu_encoder_args + ['-c:a', 'aac', '-b:a', '256k', final_output]

                return_code_cpu = self.run_ffmpeg_with_progress(
                    ffmpeg_cmd_cpu, segment_duration, task_id, status_lbl, prog_bar,
                    "Etape 2/2 : Conversion CPU (Secours)..."
                )

                if self.active_tasks[task_id]['cancel_requested']:
                    raise Exception("Annulé par l'utilisateur")
                if return_code_cpu != 0:
                    raise Exception("Le transcodage video a echoue.")

            if os.path.exists(temp_output):
                os.remove(temp_output)

            self.ui(lambda: prog_bar.set(1.0))
            self.ui(lambda: status_lbl.configure(text="Termine ! Fichier pret pour Premiere Pro.", text_color="#2ecc71"))

            self.ui(lambda: action_btn.configure(
                text="Ouvrir le dossier", state="normal", fg_color="#27ae60",
                hover_color="#2ecc71", text_color="#FFFFFF",
                command=lambda: self.open_file_folder(final_output)
            ))

        except Exception as e:
            if self.active_tasks.get(task_id, {}).get('cancel_requested'):
                self.ui(lambda: status_lbl.configure(text="Telechargement annulé par l'utilisateur.", text_color="#e67e22"))
                self.ui(lambda: action_btn.configure(text="Annulé", state="disabled", fg_color="#3A3A3A", text_color="#A0A0A0"))
            else:
                msg = str(e)
                self.ui(lambda m=msg: status_lbl.configure(text=f"Erreur : {m}", text_color="#e74c3c"))
                self.ui(lambda: action_btn.configure(text="Echec", state="disabled", fg_color="#3A3A3A", text_color="#A0A0A0"))

            self.ui(lambda: prog_bar.set(0))

            try:
                if temp_output and os.path.exists(temp_output):
                    os.remove(temp_output)
                if final_output and os.path.exists(final_output):
                    os.remove(final_output)
                if temp_output and os.path.exists(temp_output + ".part"):
                    os.remove(temp_output + ".part")
            except Exception:
                pass

        finally:
            if task_id in self.active_tasks:
                del self.active_tasks[task_id]

    def open_file_folder(self, path):
        if sys.platform.startswith("win"):
            subprocess.run(["explorer", "/select,", os.path.normpath(path)])
        elif sys.platform.startswith("darwin"):
            subprocess.run(["open", "-R", path])


if __name__ == "__main__":
    app = RobloaderApp()
    app.mainloop()
