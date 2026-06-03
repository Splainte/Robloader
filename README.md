# ⚙️ Robloader — YouTube vers H265

Robloader télécharge des vidéos YouTube (entières ou **extraits via timecodes**) et les
convertit instantanément en **HEVC (H.265)** dans un conteneur MP4 optimisé pour un import
fluide dans **Adobe Premiere Pro**.

Interface graphique sombre (CustomTkinter), file d'attente multi-téléchargements, annulation
à la volée, encodage GPU avec repli CPU automatique.

---

## 🚀 Fonctionnalités

- **Qualité MAX** — récupère le meilleur flux disponible (1080p / 4K) sans plafond de résolution.
- **Découpe au timecode** — télécharge uniquement la portion `Début → Fin` (formats `MM:SS` ou `HH:MM:SS`).
- **Optimisation Premiere Pro** — conversion HEVC GPU (NVIDIA NVENC / Apple VideoToolbox) avec repli CPU (`libx265`).
- **File d'attente** — plusieurs téléchargements en parallèle, chacun annulable.
- **Cookies optionnels** — gère les vidéos à accès restreint / âge si un `cookies.txt` est présent.

---

## 📦 Prérequis

| Dépendance | Rôle | Obligatoire |
|---|---|---|
| **Python 3.9+** | exécution | ✅ |
| **yt-dlp** (à jour) | extraction YouTube | ✅ |
| **customtkinter** | interface | ✅ |
| **ffmpeg** + **ffprobe** | découpe + ré-encodage HEVC + lecture des métadonnées | ✅ **les deux** binaires, à côté du script / bundlés |
| **Deno** (+ scripts EJS) | résout le `nsig` du player YouTube | ⚠️ **requis pour la 4K / 1440p** — sans lui, repli auto en 1080p |

Installation des paquets Python :

```bash
pip install -U "yt-dlp[default]" customtkinter
```

> ⚠️ **Garder yt-dlp à jour.** YouTube change ses protections régulièrement ; une version
> ancienne casse l'extraction. Au besoin : `yt-dlp --update-to nightly`.

### Deno + scripts EJS (requis pour la 4K)

Depuis 2025, yt-dlp **ne résout plus le `nsig`** (le script anti-bot du player YouTube) en Python
pur : il exécute le vrai JavaScript de YouTube via un **moteur externe (Deno)** et un **script
solveur « EJS »**. Concrètement :

- La **1080p** vient souvent de formats AVC qui *ne demandent pas* le `nsig` → elle passe **sans Deno**.
- La **4K / 1440p** n'existe qu'en VP9/AV1 servis par les clients web, qui **exigent** le `nsig`.
  Sans moteur JS, ces formats sont throttlés / en 403 → typiquement `ERROR: ffmpeg exited with
  code …` sur une connexion résidentielle. **C'est la cause du « la 4K bloque, la 1080p marche ».**

Deux choses sont donc nécessaires pour la 4K :

1. **Deno** — installé, ou (pour garder l'app autonome) **placé à côté de l'exe comme ffmpeg**.
   L'app ajoute déjà son dossier au `PATH` (`os.environ["PATH"] += …`), donc yt-dlp détecte un
   `deno.exe` posé à côté automatiquement. ⚠️ ~100 Mo, ça alourdit l'exécutable.
   ```bash
   # macOS / Linux
   curl -fsSL https://deno.land/install.sh | sh
   # Windows (PowerShell)
   irm https://deno.land/install.ps1 | iex
   ```
2. **Le script solveur EJS** — Robloader l'active déjà côté code via
   `remote_components=['ejs:github']` : yt-dlp le **télécharge une seule fois** depuis GitHub puis
   le met en cache. (Nécessite donc un accès internet au premier usage — sans objet pour un
   téléchargeur YouTube.)

> ✅ Testé : `player_client` multi + `remote_components=['ejs:github']` + Deno → sélection
> `401+258` soit **2160p (4K)**, `nsig` résolu, découpe par timecode OK. Sans Deno : warning
> `n challenge solving failed` et 4K KO.

#### Repli automatique si Deno est absent

Robloader **détecte Deno au démarrage** (`shutil.which('deno')`, qui voit aussi un `deno.exe`
posé à côté de l'app). Selon le résultat :

| Deno détecté | Qualité visée | Comportement |
|---|---|---|
| ✅ oui | **MAX** (1080p/4K, `bv*+ba/b`) | `nsig` résolu via EJS, 4K débloquée |
| ❌ non | **plafonnée à 1080p** | format `bv*[height<=1080]+ba/b` (sans `nsig`) → pas de plantage 4K, + un bandeau orange dans l'UI |

Autrement dit : **sans Deno, l'app ne casse plus** — elle télécharge proprement en 1080p au lieu
de planter sur la 4K. Ajouter Deno débloque la 4K sans rien changer d'autre.

### ffmpeg + ffprobe

Les binaires `ffmpeg` **et `ffprobe`** (`ffmpeg.exe` / `ffprobe.exe` sous Windows) doivent se
trouver **dans le même dossier que `Robloader.py`** (ou être inclus par PyInstaller via
`sys._MEIPASS`). ⚠️ `ffprobe` est souvent oublié : sans lui, yt-dlp affiche
`ffprobe not found ... Unable to extract metadata`. Il est livré dans la **même archive** que
ffmpeg.

### cookies.txt (optionnel)

Pour les vidéos privées / soumises à une limite d'âge, exportez vos cookies YouTube au format
Netscape dans un fichier `cookies.txt` placé à côté de l'exécutable. Robloader le détecte
automatiquement.

---

## ▶️ Lancement

```bash
python Robloader.py
```

1. Collez l'URL YouTube.
2. (Optionnel) renseignez **Début** / **Fin** pour n'extraire qu'un segment.
3. Choisissez la **Destination** (par défaut : `~/Downloads`).
4. **Telecharger** → le fichier final `.mp4` (HEVC) est prêt pour Premiere Pro.

---

## 🔧 Comment la qualité MAX est garantie

YouTube sert des formats différents selon le « client » simulé — et c'est **décisif pour la 4K** :

- `ios` / `android` **seuls** → souvent limités au **360p**.
- `web` / `web_safari` → servent de la 4K, mais **marquée `MISSING POT`** : sans PO Token, le
  serveur renvoie **HTTP 403** sur une connexion résidentielle (le flux ne se télécharge pas).
- **`tv`** → sert la **4K / 1440p SANS PO Token** : c'est la seule source 4K fiable sans serveur
  externe. Ses rares formats DRM sont automatiquement écartés par yt-dlp.

```python
player_client     = ['tv', 'ios']     # tv = 4K sans PO Token ; ios = filet de secours
format            = 'bv*+ba/b'        # meilleure vidéo + meilleur audio, sans plafond
formats           = ['missing_pot']   # ne jette pas d'emblée les formats sans PoT
remote_components = ['ejs:github']    # script solveur nsig (via Deno) -> indispensable 4K
```

⚠️ **Le piège** : exclure `tv` (ce qu'on faisait pour éviter le DRM) supprime la seule 4K
sans PoT → il ne reste que la 4K `web` → **403**. La bonne approche est de **garder `tv`** et de
laisser yt-dlp ignorer les formats DRM.

---

## 🩺 Dépannage

> 🔎 **En cas de souci 4K, lancez d'abord le diagnostic :** `python diag_youtube.py` (à poser dans
> le même dossier que `Robloader.py` / `deno.exe`). Il dit en clair si Deno est détecté, si le
> `nsig` se résout et quelle résolution sort — sans rien télécharger.

| Symptôme | Cause | Solution |
|---|---|---|
| **La 4K → `HTTP 403 Forbidden`** puis repli 1080p | flux 4K `web` qui exige un PO Token (IP résidentielle) | utiliser le client **`tv`** (déjà fait : 4K sans PoT) ; si ça persiste, ajouter un `cookies.txt` |
| **`Permission denied …\system32\…tmp`** | dossier temp non inscriptible (app lancée en admin) | corrigé : l'app force un dossier temp valide au démarrage |
| **`ffprobe not found`** | seul `ffmpeg.exe` est présent | ajouter **`ffprobe.exe`** à côté (même archive ffmpeg) |
| **La 4K bloque / `ffmpeg exited with code …`** | `nsig` non résolu (pas de moteur JS) | **installer/bundler Deno** ; EJS déjà activé dans le code |
| Bloqué / **lent** sur « Preparation » | `nsig` à résoudre + (avant) bug de threading UI | bug de threading corrigé ; mettre yt-dlp à jour ; Deno accélère |
| Vidéo en **360p** | client mobile seul sans PO Token | déjà corrigé (stratégie multi-clients) ; garder yt-dlp à jour |
| Erreur **DRM** | seul un format protégé était proposé | la liste multi-clients fournit une alternative non-DRM ; sinon la vidéo est réellement protégée |
| `Sign in to confirm…` / vidéo restreinte | YouTube exige une session | fournir un `cookies.txt` |
| Échec de l'encodage GPU | pas de GPU NVIDIA/VideoToolbox dispo | repli CPU (`libx265`) automatique |

---

## 🛠️ Notes techniques

- L'interface est mise à jour **uniquement via le thread principal** (`self.ui(...)` →
  `after(0, …)`), car Tkinter n'est pas thread-safe : modifier un widget depuis un thread de
  téléchargement pouvait figer l'app.
- Pipeline : `yt-dlp` (téléchargement / découpe) → `ffmpeg` (ré-encodage HEVC) → fichier final ;
  le fichier temporaire `temp_<id>.mp4` est supprimé à la fin (ou en cas d'échec/annulation).
- Qualité adaptative : `has_js_runtime()` teste la présence de Deno au démarrage ; le format visé
  (`bv*+ba/b` ou repli `bv*[height<=1080]+ba/b`) et l'activation d'EJS en découlent. Pas de Deno =
  pas de 4K, mais pas de crash non plus.
