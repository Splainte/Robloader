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
| **Deno** | accélère le calcul du `nsig` (anti-blocage « Preparation ») | ⭐ fortement recommandé |

Installation des paquets Python :

```bash
pip install -U "yt-dlp[default]" customtkinter
```

> ⚠️ **Garder yt-dlp à jour.** YouTube change ses protections régulièrement ; une version
> ancienne casse l'extraction. Au besoin : `yt-dlp --update-to nightly`.

### Deno (recommandé)

Sur les **vidéos longues**, yt-dlp doit résoudre le `nsig` (un gros script JS du player YouTube).
En Python pur c'est lent → l'app peut rester longtemps sur « Preparation ». Installer
[**Deno**](https://deno.com) fait que yt-dlp l'utilise comme moteur JS et rend cette étape quasi
instantanée.

```bash
# macOS / Linux
curl -fsSL https://deno.land/install.sh | sh
# Windows (PowerShell)
irm https://deno.land/install.ps1 | iex
```

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
player_client = ['default', '-tv', 'web_safari', 'ios']   # -tv = on retire le client DRM
format        = 'bv*+ba/b'                                 # meilleure vidéo + meilleur audio, sans plafond
formats       = ['missing_pot']                            # garde les formats sans PO Token
```

Cela évite à la fois le 360p (client mobile seul) et les blocages DRM (client TV, retiré via `-tv`).

---

## 🩺 Dépannage

| Symptôme | Cause | Solution |
|---|---|---|
| Bloqué sur **« Preparation »** sur les vidéos longues | résolution `nsig` lente en Python pur | **Installer Deno** + mettre yt-dlp à jour |
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
