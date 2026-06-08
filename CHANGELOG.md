# Changelog

Toutes les évolutions notables de Robloader.

## [1.0.4] — 2026-06-08

### Corrigé
- **Téléchargement « à moitié » sur certaines vidéos** (faux message « problème de cookies »,
  vidéo téléchargée mais **sans le son**, fichiers temporaires 4K webm + 1080p mp4 laissés
  derrière — remonté par Robin/Vic sur `2s_WoPudEKY`). Cause réelle : YouTube applique sur
  certaines vidéos un **PO Token sur les pistes audio** des clients utilisés (`web_embedded`,
  `tv`, `ios`) → l'audio renvoyait un **HTTP 403** pendant que la vidéo passait, d'où le
  `bv*+ba` qui échouait puis repliait en 1080p (re-403). Le réglage `formats=missing_pot`
  aggravait le tout en **gardant** ces formats audio morts. Correctif : on retire `missing_pot`
  (les formats morts sont jetés) et on bascule sur les clients `default` + **`android_vr`**
  (audio+vidéo sans PoT ni DRM), `web_embedded` restant pour la 4K-avec-cookies. Vérifié :
  la vidéo se télécharge désormais **avec le son**.

### Ajouté
- **Build macOS Intel (x86_64)** en plus d'Apple Silicon. Chaque release publie maintenant
  `Robloader-macos-arm64.dmg` (M1/M2/M3…) **et** `Robloader-macos-intel.dmg` (Mac Intel).

## [1.0.3] — 2026-06-05

### Corrigé
- **Dossier de téléchargement par défaut = le VRAI dossier Téléchargements de l'OS**, même s'il a
  été déplacé sur un autre disque. Avant, le chemin était reconstruit à la main (`~/Downloads`),
  ce qui recréait un dossier « Downloads » en doublon au lieu d'utiliser le dossier relocalisé
  (remonté par Robin). Sous Windows le chemin réel est lu via l'API système (FOLDERID_Downloads),
  sous Linux via `xdg-user-dirs`.

### Ajouté
- **Erreur cookies plus claire + bouton « Réparer »** : quand YouTube exige une connexion
  (vérification anti-robot, restriction d'âge, vidéo réservée aux membres, 403), le message
  l'explique et un bouton **« Réparer »** ouvre le mode d'emploi cookies.
- **Mise à jour automatique** : au lancement, l'app interroge l'API GitHub Releases (dépôt public,
  sans authentification). Si une version plus récente existe, une bannière **« Mise à jour
  disponible »** s'affiche. Le bouton **« Mettre à jour »** télécharge l'installeur de la
  plateforme (`Robloader-Setup.exe` / `Robloader.dmg`) puis le lance — sous Windows l'app se ferme
  pour laisser l'installeur écraser les fichiers. Bouton **« Plus tard »** pour ignorer. Non
  bloquant, échoue en silence si hors ligne.

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
