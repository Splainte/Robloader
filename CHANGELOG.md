# Changelog

Toutes les évolutions notables de Robloader.

## [1.0.2] — 2026-06-03

### Corrigé
- **Extraction par timecodes qui bloquait à l'infini** (« Extraction du segment… »). Sur les
  sessions YouTube en **SABR**, l'ancienne méthode faisait télécharger la plage par ffmpeg, qui
  restait bloqué faute d'URL directe. Désormais : **téléchargement complet en natif puis découpe
  locale** (précise à la seconde). ⚖️ Revers : la vidéo entière est téléchargée avant la découpe
  → pour un extrait d'une longue vidéo, préférer **720p**.

### Ajouté
- **Nom de la chaîne dans le fichier** : `Titre - Chaîne.mp4`.
- Format de sortie **« Sous-titres seuls (.srt) »** (aucune vidéo téléchargée).

## [1.0.1] — 2026-06-03

### Corrigé
- **Sous-titres et miniature** désormais fiables : écrits pendant le téléchargement (au lieu d'un
  post-traitement qui échouait silencieusement).

## [1.0.0] — 2026-06-03

Première version **multi-plateforme** (Windows + macOS), distribuée en `.exe`/installeur et `.dmg`,
avec ffmpeg, ffprobe et Deno **embarqués** (rien à installer).

### Ajouté
- **Sélecteur de qualité** (Max/4K, 1440p, 1080p, 720p, 480p), **mémorisé** d'une session à l'autre.
- **Formats de sortie** : HEVC (Premiere), **ProRes** (.mov), **Audio MP3**, **Audio WAV**.
- **Sous-titres .srt** et **miniature** en option.
- **Transcodage intelligent** : le H.264 (≤ 1080p) est gardé tel quel → import Premiere immédiat,
  sans ré-encodage ; seul le VP9/AV1 (4K) est converti en H.265.
- **Cookies automatiques** (Firefox / Chrome / Edge / Safari) pour débloquer la 4K, avec repli sur
  un `cookies.txt`.
- **Plusieurs URLs d'un coup** (batch) + bouton **« Nettoyer la liste »**.
- **Auto-update de yt-dlp** en tâche de fond → l'app reste fonctionnelle quand YouTube change.
- Interface repensée + icône, et une **version macOS** (encodage VideoToolbox).

### Corrigé
- 4K débloquée (résolution du `nsig` via Deno + PO Token via cookies).
- Plus de plantage `Permission denied …\system32` (dossier temporaire forcé inscriptible).
- Plus d'UI figée (mises à jour d'interface rendues thread-safe).
- `ffprobe` désormais embarqué (fini `ffprobe not found`).
- Résolu le conflit de merge qui empêchait le fichier de s'exécuter.

[1.0.2]: https://github.com/Splainte/Robloader/releases/tag/v1.0.2
[1.0.1]: https://github.com/Splainte/Robloader/releases/tag/v1.0.1
[1.0.0]: https://github.com/Splainte/Robloader/releases/tag/v1.0.0
