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
| **ffmpeg** | découpe + ré-encodage HEVC | ✅ (placé à côté du script / bundlé) |
| **Deno** (+ scripts EJS) | résout le `nsig` du player YouTube | ⚠️ **requis pour la 4K / 1440p** (la 1080p passe sans) |

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

### ffmpeg

Le binaire `ffmpeg` (`ffmpeg.exe` sous Windows) doit se trouver **dans le même dossier que
`Robloader.py`** (ou être inclus par PyInstaller via `sys._MEIPASS` pour la version packagée).

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

YouTube sert des formats différents selon le « client » simulé :

- `ios` / `android` **seuls** → souvent limités au **360p** (sans PO Token).
- `tv` **seul** → peut renvoyer des formats **DRM** non téléchargeables.

Robloader interroge donc **plusieurs clients à la fois** et laisse yt-dlp agréger puis choisir
le meilleur format **non-DRM** disponible :

```python
player_client     = ['default', '-tv', 'web_safari', 'ios']   # -tv = on retire le client DRM
format            = 'bv*+ba/b'                                 # meilleure vidéo + meilleur audio, sans plafond
formats           = ['missing_pot']                            # garde les formats sans PO Token
remote_components = ['ejs:github']                             # script solveur nsig -> debloque la 4K
```

Cela évite le 360p (client mobile seul), les blocages DRM (client TV, retiré via `-tv`), et —
grâce à Deno + EJS — débloque la **4K/1440p** dont les flux exigent la résolution du `nsig`.

---

## 🩺 Dépannage

| Symptôme | Cause | Solution |
|---|---|---|
| **La 4K bloque / `ffmpeg exited with code …`** (mais la 1080p marche) | `nsig` non résolu → flux 4K throttlés / 403 | **installer/bundler Deno** ; le script EJS est déjà activé dans le code (`remote_components`) |
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
