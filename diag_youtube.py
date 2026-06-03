"""
Diagnostic Robloader / yt-dlp — pourquoi la 4K ne marche pas ?

A poser DANS LE MEME DOSSIER que Robloader.py / ffmpeg.exe / deno.exe, puis lancer :
    python diag_youtube.py
(ou, depuis l'exe gele, peu importe : ce script teste l'environnement Python local.)

Il ne telecharge PAS la video : il interroge juste YouTube et dit ou ca coince.
"""
import os
import sys
import shutil
import tempfile
import subprocess

# Reproduit l'injection de PATH faite par Robloader (pour detecter un deno.exe local)
HERE = os.path.dirname(os.path.abspath(__file__))
os.environ["PATH"] += os.pathsep + HERE

URL = sys.argv[1] if len(sys.argv) > 1 else "https://www.youtube.com/watch?v=aqz-KE-bpKQ"

print("=" * 60)
print("DIAGNOSTIC ROBLOADER")
print("=" * 60)
print(f"Python        : {sys.version.split()[0]}")

try:
    import yt_dlp
    print(f"yt-dlp        : {yt_dlp.version.__version__}")
except Exception as e:
    print(f"yt-dlp        : INTROUVABLE ({e})")
    print(">>> Installez/maj : pip install -U yt-dlp")
    sys.exit(1)

deno = shutil.which("deno")
print(f"deno detecte  : {deno or 'NON'}")
if deno:
    try:
        v = subprocess.run([deno, "--version"], capture_output=True, text=True, timeout=15)
        print(f"deno version  : {v.stdout.splitlines()[0] if v.stdout else '?'}")
    except Exception as e:
        print(f"deno version  : erreur a l'execution -> {e}")
        deno = None

# Dossier temp : cause frequente du "(Errno 13) Permission denied ...tmp" / echec 4K dans l'app
sys_tmp = tempfile.gettempdir()
tmp_ok = False
try:
    _p = tempfile.NamedTemporaryFile(delete=True)
    _p.close()
    tmp_ok = True
except Exception as e:
    tmp_err = e
print(f"cwd           : {os.getcwd()}")
print(f"dossier temp  : {sys_tmp}")
print(f"temp ecrivable: {'OUI' if tmp_ok else 'NON -> ' + repr(tmp_err)}")
if not tmp_ok or 'system32' in sys_tmp.lower():
    print("  ⚠ Temp inutilisable : c'est CA qui fait planter la 4K dans l'app (pas le diag).")
    print("    -> corrige par la nouvelle version (force un dossier temp inscriptible).")
print("-" * 60)


class WarnLogger:
    def __init__(self):
        self.warnings = []
    def debug(self, m):
        pass
    def info(self, m):
        pass
    def warning(self, m):
        self.warnings.append(m)
    def error(self, m):
        self.warnings.append("ERROR: " + m)


log = WarnLogger()
opts = {
    "quiet": True,
    "skip_download": True,
    "logger": log,
    "remote_components": ["ejs:github"],
    "extractor_args": {"youtube": {
        "player_client": ["default", "-tv", "web_safari", "ios"],
        "formats": ["missing_pot"],
    }},
    "format": "bv*+ba/b",
    "format_sort": ["res", "fps", "br"],
}

print("Interrogation de YouTube (config Robloader)...")
try:
    with yt_dlp.YoutubeDL(opts) as y:
        info = y.extract_info(URL, download=False)
    reqs = info.get("requested_formats") or [info]
    maxh = max((f.get("height") or 0) for f in reqs)
    ids = "+".join(str(f.get("format_id")) for f in reqs)
    print(f"Format choisi : {ids}  ->  {maxh}p")
    nsig_fail = any("challenge" in w.lower() or "nsig" in w.lower() for w in log.warnings)
    print(f"nsig resolu   : {'NON (challenge solving failed)' if nsig_fail else 'OUI'}")
    print("-" * 60)
    if maxh >= 1440 and not nsig_fail:
        print("VERDICT : ✅ tout est bon, la 4K/1440p doit fonctionner.")
    elif nsig_fail or maxh < 1440:
        print("VERDICT : ❌ la haute resolution est bloquee. Causes possibles :")
        if not deno:
            print("  - Deno n'est PAS detecte -> posez deno.exe a cote de ce script (et de l'exe).")
        else:
            print("  - Deno OK mais nsig non resolu -> le script solveur EJS n'a pas pu se telecharger.")
            print("    Verifiez l'acces a github.com (firewall/proxy d'entreprise ?).")
        print("  - yt-dlp peut-etre trop ancien -> pip install -U yt-dlp")
    if log.warnings:
        print("-" * 60)
        print("Warnings yt-dlp :")
        for w in log.warnings[:8]:
            print("  -", w[:160])
except Exception as e:
    print(f"ECHEC extraction : {e!r}")
    if log.warnings:
        for w in log.warnings[:8]:
            print("  -", w[:160])

print("=" * 60)
print("Copiez-collez toute cette sortie pour le diagnostic.")
