# Changelog

Toutes les évolutions notables de Robloader.

## [1.1.0] — 2026-06-10

### Ajouté
- **Multi-sources** : téléchargement depuis **TikTok, Instagram, X (Twitter) et Weibo** en plus de YouTube.
- **Interface réactive à la source** : en collant un lien, l'app détecte le site et **fond enchaîné**
  la couleur des boutons et des cases vers la charte du site (rouge YouTube, `#FE2C55` TikTok,
  magenta Instagram, noir X, rouge Weibo).
- **Options adaptées au site** : les cases sous-titres / miniature et le sélecteur de qualité se
  masquent automatiquement quand le site ne les concerne pas.

### Modifié
- Pipeline yt-dlp aiguillé par site : nsig/Deno réservé à YouTube, meilleur flux disponible ailleurs.
- En-tête épuré : suppression du sous-titre « YouTube → fichier prêt pour Premiere Pro » (hors sujet
  en multi-sources).

### Corrigé
- Champ URL : le texte d'invite grisé pouvait rester collé au lien lors d'un remplacement
  (Cmd+A puis Cmd+V) → « Colle u‹lien›n lien… ». Corrigé (plus de reconfiguration dynamique du
  placeholder dans CTkEntry).

## [1.0.8] — 2026-06-09

### Corrigé
- **Label cookies affichant un navigateur non installé (macOS)** : la détection considérait un
  navigateur Chromium présent dès que son dossier de base existait, ce qui pouvait afficher
  « cookies chrome/edge ✓ » alors qu'Edge n'était pas installé (dossier `Microsoft Edge` résiduel).
  La détection teste désormais le fichier `Local State`, créé uniquement après un vrai premier
  lancement. Le bandeau d'état liste aussi tous les navigateurs détectés (avant : les 2 premiers).
- **Icône qui grossissait dans le dock à l'ouverture (macOS, Intel + Apple Silicon)** : `iconphoto()`
  remplaçait l'icône `.icns` du bundle (dessinée sur la grille Apple) par un PNG plein cadre, plus
  gros. On n'appelle plus `iconphoto()` sur macOS : l'icône du `.app` reste cohérente, ouverte
  comme fermée.

### Interne
- `CFBundleShortVersionString` / `CFBundleVersion` du `.app` sont désormais lus depuis `APP_VERSION`
  (`Robloader.py`) au lieu d'être codés en dur (ils restaient bloqués à `1.0.0`).

## [1.0.7] — 2026-06-09

### Corrigé
- **Vérification de mise à jour silencieuse sur macOS** : le Python bundlé par PyInstaller ne trouvait
  pas les certificats SSL système → `urlopen` levait une `SSLCertVerificationError` avalée
  silencieusement → la bannière de mise à jour n'apparaissait jamais sur Mac. Corrigé en utilisant
  `certifi` (bundle de certificats CA autonome) pour créer le contexte SSL des appels réseau
  (vérification de release GitHub + téléchargement de mise à jour yt-dlp). `certifi` est désormais
  embarqué dans le bundle PyInstaller via `collect_data_files('certifi')`.

## [1.0.6] — 2026-06-09

### Ajouté
- **Case « Transcodage »** sur la ligne Sortie (cochée par défaut). En la décochant, le transcodage
  ffmpeg est désactivé : yt-dlp livre le fichier dans son format natif (`.webm`, `.mkv`, `.mp4`…)
  sans aucune conversion — téléchargement plus rapide, fichier potentiellement illisible dans Premiere.
  Si des timecodes sont fournis, la découpe est effectuée par copie de flux (`-c copy`) sans ré-encodage.
- **Menu Sortie adaptatif** : quand le transcodage est désactivé, les options HEVC et ProRes disparaissent
  (sans objet) et le menu se réduit à `Vidéo (natif)`, Audio MP3, Audio WAV, Sous-titres seuls.
  Basculer la case remet automatiquement le menu à jour et corrige la sélection si besoin.

### Modifié
- Renommage des sorties : `HEVC (Premiere)` → **HEVC**, `ProRes (montage)` → **ProRes**.

## [1.0.5] — 2026-06-08

### Ajouté
- **Vraie compatibilité Mac Intel, via un DMG Universal2 unique.** En 1.0.4, le build Intel
  passait par un runner GitHub `macos-13` qui n'a jamais démarré (les runners macOS Intel sont en
  cours de retrait chez GitHub → job bloqué en file d'attente). On produit désormais, depuis le
  runner Apple Silicon, un seul **`Robloader-macos.dmg` universal2** qui tourne **nativement sur
  Intel ET Apple Silicon** : Python universal2 (python.org), binaires embarqués (ffmpeg, ffprobe,
  deno) fusionnés en universels (`lipo`), et empaquetage PyInstaller `target_arch=universal2`.
  Plus de choix de version à faire côté utilisateur.

### Modifié
- La mise à jour auto sur Mac télécharge maintenant `Robloader-macos.dmg` (au lieu de l'ancien
  nom `Robloader.dmg`).

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
- Tentative de build macOS Intel (x86_64) via un runner `macos-13`. **N'a pas abouti** (runner
  Intel jamais alloué côté GitHub) → seul `Robloader-macos-arm64.dmg` a été publié pour cette
  version. Corrigé en 1.0.5 par un DMG universal2.

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

[1.1.0]: https://github.com/Splainte/Robloader/releases/tag/v1.1.0
[1.0.8]: https://github.com/Splainte/Robloader/releases/tag/v1.0.8
[1.0.7]: https://github.com/Splainte/Robloader/releases/tag/v1.0.7
[1.0.6]: https://github.com/Splainte/Robloader/releases/tag/v1.0.6
[1.0.5]: https://github.com/Splainte/Robloader/releases/tag/v1.0.5
[1.0.4]: https://github.com/Splainte/Robloader/releases/tag/v1.0.4
[1.0.3]: https://github.com/Splainte/Robloader/releases/tag/v1.0.3
[1.0.2]: https://github.com/Splainte/Robloader/releases/tag/v1.0.2
[1.0.1]: https://github.com/Splainte/Robloader/releases/tag/v1.0.1
[1.0.0]: https://github.com/Splainte/Robloader/releases/tag/v1.0.0
