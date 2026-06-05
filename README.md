# ⚙️ Robloader — YouTube vers H265

Robloader télécharge des vidéos YouTube (entières ou **extraits via timecodes**) et les
convertit instantanément en **HEVC (H.265)** dans un conteneur MP4 optimisé pour un import
fluide dans **Adobe Premiere Pro**.

Interface graphique sombre (CustomTkinter), file d'attente multi-téléchargements, annulation
à la volée, encodage GPU avec repli CPU automatique.

---

## 🚀 Fonctionnalités

- **Sélecteur de qualité** — menu déroulant (Max/4K, 1440p, 1080p, 720p, 480p), **mémorisé** d'une session à l'autre.
- **Découpe au timecode** — télécharge uniquement la portion `Début → Fin` (`MM:SS` ou `HH:MM:SS`), ou la vidéo entière si vide.
- **Transcodage H.265 intelligent** — ne ré-encode **que** les sources VP9/AV1 (4K). Le **H.264 (1080p et moins) est gardé tel quel** : import Premiere immédiat, sans attente d'encodage.
- **Optimisation Premiere Pro** — encodage HEVC GPU (NVIDIA NVENC / Apple VideoToolbox) avec repli CPU (`libx265`).
- **Cookies automatiques** — lit les cookies de **Chrome/Safari** si connecté à YouTube (4K sans manip), ou un `cookies.txt` posé à côté de l'app.
- **Aide cookies intégrée** — si YouTube exige une connexion (vérification anti-robot, âge, vidéo réservée aux membres, 403), le message l'explique en clair et un bouton **« Réparer »** ouvre le mode d'emploi.
- **Mise à jour automatique** — au lancement, l'app vérifie auprès de l'API GitHub Releases s'il existe une version plus récente et affiche un bandeau **« Mise à jour disponible »** ; le bouton **« Mettre à jour »** télécharge et lance l'installeur de la plateforme. Non bloquant, silencieux si hors ligne.
- **Vrai dossier Téléchargements** — la destination par défaut est le **dossier Téléchargements réel du système** (même s'il a été déplacé sur un autre disque), pas un `~/Downloads` reconstruit à la main.
- **File d'attente** — plusieurs téléchargements en parallèle, chacun annulable, + bouton **« Nettoyer la liste »**.
- **Multi-plateforme** — Windows (NVENC) et **macOS** (VideoToolbox, `.app` + `.dmg`).

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

### cookies.txt (clé de la 4K sur certaines sessions)

Au-delà des vidéos privées / limitées en âge, le `cookies.txt` est **le déblocage le plus fiable
pour la 4K**. Pourquoi : YouTube protège les flux 4K de deux façons selon le client —

- le client **`tv`** peut tomber dans une **expérimentation DRM** (DRM appliqué à tout —
  [yt-dlp #12563](https://github.com/yt-dlp/yt-dlp/issues/12563)) → 4K écartée ;
- les clients **web** servent la 4K mais exigent un **PO Token** → `HTTP 403` sur IP résidentielle.

**Être authentifié (cookies) fait sauter ces deux verrous** : le client web devient « de confiance »
et délivre la 4K sans 403. C'est plus simple qu'un serveur de PO Token.

**Le plus simple — cookies automatiques** : si vous êtes connecté à YouTube dans **Chrome** ou
**Safari**, Robloader lit les cookies du navigateur tout seul (rien à faire). La ligne d'état en
haut affiche alors `cookies chrome ✓`.

**Sinon — fichier cookies.txt** : avec une extension type *« Get cookies.txt LOCALLY »*, connecté à
YouTube, exportez au format Netscape dans un **`cookies.txt`** placé **à côté de l'app**. Il est
prioritaire sur les cookies navigateur. `diag_youtube.py` le détecte aussi.

> 🧩 Alternative lourde si les cookies ne suffisent pas : un **fournisseur de PO Token**
> ([bgutil-ytdlp-pot-provider](https://github.com/Brainicism/bgutil-ytdlp-pot-provider)), à faire
> tourner en parallèle. Plus robuste, mais ce n'est plus une app autonome.

---

## ▶️ Lancement

```bash
python Robloader.py
```

1. Collez l'URL YouTube.
2. (Optionnel) renseignez **Début** / **Fin** pour n'extraire qu'un segment.
3. Choisissez la **Destination** (par défaut : votre **dossier Téléchargements système**, ou le dernier dossier utilisé).
4. **Telecharger** → le fichier final `.mp4` (HEVC) est prêt pour Premiere Pro.

---

## 🍎 Version macOS

Le code est cross-platform (encodage `hevc_videotoolbox`, ouverture Finder, icône, etc.).
Pour produire un **`Robloader.app`** + un **`.dmg`** distribuable :

```bash
# Sur un Mac. Tk 8.6+/9.0 REQUIS (le Python système est en Tk 8.5 -> UI grise) :
brew install python-tk
"$(brew --prefix)/bin/python3" -m venv .venv && source .venv/bin/activate
./build_macos.sh
open dist/Robloader.app
```

Le script (`build_macos.sh`) :
1. détecte l'architecture (**Apple Silicon `arm64`** ou **Intel `x86_64`**) ;
2. installe les dépendances Python (`pyinstaller`, `customtkinter`, `yt-dlp`) ;
3. télécharge **Deno** + **ffmpeg/ffprobe statiques** (autonomes) dans `bin/` ;
4. construit le `.app` (Deno + ffmpeg + ffprobe + icône **embarqués**) ;
5. produit un **`dist/Robloader.dmg`** prêt à distribuer (glisser l'app dans Applications).

### Points d'attention macOS

- **Tk 8.6+ / 9.0 obligatoire** : le Python *système* (Tk 8.5, déprécié) rend l'UI **toute grise**.
  Utilisez un Python Homebrew (`brew install python-tk`) — d'où le venv dans les commandes ci-dessus.
- **ffmpeg statique** : le script récupère des binaires **autonomes** (pas de dylibs) → l'app
  tourne sur **n'importe quel Mac**. Repli Homebrew seulement si le téléchargement échoue (et là,
  ça ne tournera que sur *ta* machine).
- **Gatekeeper** : l'app n'est pas signée → au 1er lancement, **clic droit ▸ Ouvrir** (ou
  `xattr -dr com.apple.quarantine dist/Robloader.app`). Distribution sans avertissement = signer +
  notariser (compte Apple Developer, `codesign` / `notarytool`) — non configuré ici.
- **cookies** : dans un `.app`, « à côté de l'app » est *hors* du bundle ; Robloader cherche aussi
  `~/Library/Application Support/Robloader/`. Le plus simple reste les **cookies navigateur auto**.
- **GPU** : encodage HEVC via `hevc_videotoolbox` (matériel Apple) ; pas de NVENC sur Mac.

## 🔧 Comment la qualité MAX est garantie

YouTube sert des formats différents selon le « client » simulé — et c'est **décisif pour la 4K** :

- `ios` / `android` **seuls** → souvent limités au **360p**.
- `web` / `web_safari` → servent de la 4K, mais **marquée `MISSING POT`** : sans PO Token, le
  serveur renvoie **HTTP 403** sur une connexion résidentielle (le flux ne se télécharge pas).
- **`web_embedded`** → sert la 4K (idéal **avec cookies**, voir plus haut).
- **`tv`** → 4K sans PO Token, sauf si la session est dans l'**expérimentation DRM** (#12563).

```python
player_client     = ['web_embedded', 'tv', 'ios']   # 4K via web_embedded (cookies) ou tv ; ios = secours
format            = 'bv*+ba/b'                       # meilleure vidéo + meilleur audio, sans plafond
formats           = ['missing_pot']                  # ne jette pas d'emblée les formats sans PoT
remote_components = ['ejs:github']                   # script solveur nsig (via Deno) -> indispensable 4K
```

⚠️ **Aucune combinaison de clients ne garantit la 4K à elle seule** quand une session cumule
DRM-sur-`tv` **et** PoT-sur-`web`. Le vrai déblocage est alors **`cookies.txt`** (voir la section
dédiée). À défaut, yt-dlp écarte le DRM, le 403 déclenche le repli, et on obtient du **1080p propre**.

---

## 🩺 Dépannage

> 🔎 **En cas de souci 4K, lancez d'abord le diagnostic :** `python diag_youtube.py` (à poser dans
> le même dossier que `Robloader.py` / `deno.exe`). Il dit en clair si Deno est détecté, si le
> `nsig` se résout et quelle résolution sort — sans rien télécharger.

| Symptôme | Cause | Solution |
|---|---|---|
| **La 4K → `HTTP 403`** ou **`DRM protected` sur tv (#12563)** puis repli 1080p | session filtrée : `web` exige un PoT, `tv` est sous expérimentation DRM | **ajouter un `cookies.txt`** (déblocage le plus fiable) ; sinon fournisseur de PO Token |
| **`Permission denied …\system32\…tmp`** | dossier temp non inscriptible (app lancée en admin) | corrigé : l'app force un dossier temp valide au démarrage |
| **`ffprobe not found`** | seul `ffmpeg.exe` est présent | ajouter **`ffprobe.exe`** à côté (même archive ffmpeg) |
| **La 4K bloque / `ffmpeg exited with code …`** | `nsig` non résolu (pas de moteur JS) | **installer/bundler Deno** ; EJS déjà activé dans le code |
| Bloqué / **lent** sur « Preparation » | `nsig` à résoudre + (avant) bug de threading UI | bug de threading corrigé ; mettre yt-dlp à jour ; Deno accélère |
| Vidéo en **360p** | client mobile seul sans PO Token | déjà corrigé (stratégie multi-clients) ; garder yt-dlp à jour |
| Erreur **DRM** | seul un format protégé était proposé | la liste multi-clients fournit une alternative non-DRM ; sinon la vidéo est réellement protégée |
| `Sign in to confirm…` / vidéo restreinte | YouTube exige une session | cliquer sur **« Réparer »** (ouvre le mode d'emploi) puis fournir des cookies (navigateur connecté ou `cookies.txt`) |
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
- **Mise à jour** : au lancement, `_check_update_async()` interroge `releases/latest` de l'API
  GitHub (le dépôt est **public** → aucun token) et compare le dernier tag à `APP_VERSION`. Si plus
  récent, le bandeau propose de télécharger l'asset (`Robloader-Setup.exe` / `Robloader.dmg`) et de
  le lancer. ⚠️ **À chaque release, bumper `APP_VERSION` dans `Robloader.py` ET `AppVersion` dans
  `installer/Robloader.iss`**, puis `git tag vX.Y.Z` — sinon la comparaison de version ne se
  déclenche pas et la CI ne reflète pas la bonne version.
- **Build CI** (`.github/workflows/build.yml`) : un `git push` de tag `vX.Y.Z` build Windows
  (`.exe` + installeur Inno Setup) et macOS (`.dmg`) et les attache à la Release. Les binaires
  embarqués (Deno, ffmpeg/ffprobe) sont téléchargés au build ; pour ffmpeg sous Windows on vise le
  **tag littéral** `releases/download/latest` de BtbN (sa release `latest` est une *pre-release*,
  donc le raccourci `releases/latest/download` renvoie 404).
