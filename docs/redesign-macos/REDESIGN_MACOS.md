# Redesign macOS de Robloader : piste D3 « volet à gauche »

> Rédigé le 15/09/2026 à la fin d'une session d'exploration menée depuis le projet Copiteur.
> **Statut : direction validée par Robin sur maquette. Rien n'est implémenté.**
> Windows n'est pas concerné : son interface actuelle reste telle quelle.

---

## 1. En une phrase

Sur macOS, Robloader adopte une vraie barre d'outils native (lien + Télécharger), un volet latéral gauche en Liquid Glass **toujours ouvert** qui regroupe tous les réglages, et une file de téléchargements qui occupe le reste de la fenêtre.

---

## 2. D'où on part (historique utile)

| Élément | État |
|---|---|
| Build macOS lié au SDK macOS 27 | ✅ Mergé (PR #32) : runner GitHub `xcode-27` dans `.github/workflows/build.yml`. Sans lui, macOS 27 affiche l'ancien design des feux tricolores et du chrome natif. |
| Branche `feat/liquid-glass` | POC de juin 2026 : `NSGlassEffectView` en fond plein cadre. Fonctionne mais nuit à la lisibilité. **À garder** (demande de Robin). |
| Branche `feat/presentation-liquid-glass` (worktree `../Robloader-glass`) | Menu macOS en français + menu « Présentation > Liquid Glass » qui bascule à chaud le fond entre vibrancy et verre plein cadre. Testé par Robin : « ça marche, mais c'est pas très beau ». **Non mergée.** Son code est réutilisable (voir §6). |
| UI actuelle sur `main` | Barre de titre HTML, ligne lien/qualité/destination/Télécharger, ligne Extrait, ligne options, ligne d'état, bannière de mise à jour, file de tâches. Fond vibrancy. |

### Le cheminement de design (pour ne pas refaire le débat)
1. **Banc d'essai de six pistes** : vibrancy actuelle, verre plein cadre, barres en verre (A), îlots flottants (B), contenu sous le verre (C), barre d'outils native (D). Robin a retenu **D**.
2. **Quatre variantes de D** : D1 lien en barre, D2 lien centré avec options en popover, D3 inspecteur, D4 options flottantes en capsule. Robin a retenu **D3** avec deux changements :
   - le volet passe **à gauche** (il était à droite dans la maquette) ;
   - **aucun moyen de le masquer** : pas de bouton, pas de raccourci, toujours ouvert.

### Pourquoi pas le verre plein cadre
Apple conçoit le Liquid Glass pour la **couche de navigation** (barres, volets, contrôles flottants) posée sur un contenu **opaque**. En fond de fenêtre, derrière nos panneaux CSS, le verre ne se voit que dans les interstices et brouille la lecture. D3 met le verre là où Apple le met : barre d'outils et volet latéral.

---

## 3. Maquettes de référence

- **Fichier local, à ouvrir dans un navigateur** : `docs/redesign-macos/maquette-variantes-d.html` (copie jointe à cette note). Choisir *App : Robloader*, puis *Variante : D3 · Volet à gauche (retenue)*. Le bouton *Repérer* entoure en bleu ce qui serait natif et en orange ce qui resterait en HTML. Le curseur *Liquid Glass* imite le réglage de macOS 27.
- Versions en ligne (privées, compte claude.ai de Robin) :
  - Variantes D : https://claude.ai/artifact/LxhqJgwJN9WczTBSxekEyH
  - Banc d'essai des six pistes : https://claude.ai/artifact/VkAEDTW3eNKGeJXYDXGgoi
- ⚠️ La maquette est une **imitation web** : flou, teinte et liserés sont simulés, pas la réfraction réelle. Certaines valeurs y sont fictives (formats « MP4/MOV », vignettes colorées). **Les valeurs réelles sont celles de ce document** (§4.3).

---

## 4. Spécification de l'interface macOS

> ⚠️ Les arbitrages et exigences du **§8** (16/09/2026) prévalent sur cette section partout où ils diffèrent : libellés courts, interrupteurs sur la même ligne, mise à jour dans la barre, titre sur une ligne.

### 4.1 Anatomie

```
┌──────────────────────────────────────────────────────────────────────────┐
│ ● ● ●  Robloader              ( lien  https://youtu.be/…   YouTube  [⧉] )  [ ⬇ Télécharger ] │ ← barre d'outils NATIVE
│        2.1.7 · cookies Chrome ✓                                            │
│ ╭────────────────────────╮  File de téléchargements · 2 en cours   [Nettoyer la liste] │
│ │ QUALITÉ                │  ┌────────────────────────────────────────────────┐ │
│ │ [Max][1440p][1080p]    │  │ ▢ Test du Pixel 10 Pro…              [Annuler] │ │
│ │ [720p][480p]           │  │   Téléchargement 64 % · 12,3 Mo/s · reste 8 s  │ │
│ │ OPTIONS                │  │   ━━━━━━━━━━━━━━━━━━━░░░░░░░░░░                │ │
│ │ Extrait            (○) │  ├────────────────────────────────────────────────┤ │
│ │ Transcodage        (○) │  │ ▢ iPhone 17 Pro vs Galaxy S26…      [Afficher] │ │
│ │ Format de sortie [HEVC]│  │   Terminé · 1,2 Go                             │ │
│ │ Sous-titres (.srt) (●) │  └────────────────────────────────────────────────┘ │
│ │ Miniature          (○) │                                                    │
│ │ DESTINATION            │                                                    │
│ │ ~/Movies/Robloader     │                                                    │
│ │ [Choisir…] [Finder]    │                                                    │
│ │ ── cookies ✓ · 4K OK ──│                                                    │
│ ╰────────────────────────╯                                                    │
└──────────────────────────────────────────────────────────────────────────┘
   volet VERRE natif,                file en HTML sur fond plein,
   toujours ouvert                   défile sous la barre d'outils
```

Les trois couches :
1. **Natif (dessiné par macOS)** : barre d'outils, feux tricolores, menus contextuels, menus locaux, barre de menus, verre du volet.
2. **HTML posé sur le verre** : le contenu du volet (sections, interrupteurs, grille de qualité).
3. **HTML sur fond plein** : la file de téléchargements.

### 4.2 Barre d'outils (native)

De gauche à droite :

| Élément | Détail |
|---|---|
| Feux tricolores | Natifs, inchangés. |
| Titre + sous-titre | « Robloader » et, dessous, « version · état des cookies » (ex. `2.1.7 · cookies Chrome ✓`). Idéalement `NSWindow.title` et `NSWindow.subtitle`, sinon titre d'item. |
| Champ de lien (flexible, prend toute la place libre) | Icône lien à gauche. Le texte indicatif dépend du profil détecté (`SITE_PROFILES` dans `App.tsx`, ex. « Colle un lien YouTube ici… »). **Pastille de source** à droite dans le champ (YouTube, TikTok, Instagram, X, Weibo), mise à jour à chaque frappe. Bouton **Coller** (icône presse-papiers) en bout de champ. |
| Bouton « Télécharger » | Style **proéminent teinté** de la couleur d'accent système (bouton principal macOS 26). Icône flèche vers le bas + libellé. |

**Il n'y a pas** de bouton pour masquer le volet, ni de sélecteur de qualité ou de destination dans la barre : tout est dans le volet.

Comportements :
- **Entrée** dans le champ : lance le téléchargement.
- **⌘L** : met le focus dans le champ de lien.
- Après ajout à la file, le champ se vide et la pastille disparaît.
- Lien vide + Télécharger : message discret « Colle d'abord un lien » et focus dans le champ, sans alerte bloquante.

### 4.3 Volet latéral gauche (verre natif, toujours ouvert)

- **Position** : à gauche, sous la barre d'outils, flottant (≈10 px de marge sur les bords, coins arrondis ≈22 px, concentriques avec le coin de fenêtre).
- **Largeur fixe ≈ 290 px**. Non redimensionnable pour l'instant.
- **Toujours visible** : ni bouton, ni raccourci, ni entrée de menu pour le replier.
- **Défilement interne** si la fenêtre est basse. Le volet ne défile pas avec la file.

Sections, dans l'ordre :

**1. Qualité** : titre de section en petites capitales grises.
- Grille de boutons, 3 colonnes, un seul choix possible, le choix actif rempli de la couleur d'accent.
- Valeurs réelles (`QUALITY_LABELS`) : `Qualité max (jusqu'à 4K)` (libellé court « Max »), `1440p (QHD)`, `1080p (Full HD)`, `720p (HD)`, `480p`.
- Section affichée seulement si le profil détecté a `ladder: true` (YouTube, lien générique). Pour TikTok, Instagram, X et Weibo, remplacer la grille par une ligne grise « Qualité automatique pour ce site » plutôt que de faire sauter la mise en page.

**2. Options** : liste groupée façon Réglages Système (lignes séparées par un filet, interrupteurs à droite).

| Ligne | Contrôle | Règle |
|---|---|---|
| Extrait · « Ne récupérer qu'un passage de la vidéo » | Interrupteur | Activé, il fait apparaître juste en dessous une ligne **Début** / **Fin** (champs `MM:SS` ou `HH:MM:SS`, exemples `00:00` / `01:30`). Vide = vidéo entière. |
| Transcodage · « Réencode pour un montage fluide dans Premiere Pro » | Interrupteur | Change la liste des formats de sortie (ligne suivante). |
| Format de sortie | **Menu local natif** | Transcodage activé (`OUTPUTS_TRANSCODE`) : `HEVC`, `ProRes`, `Audio WAV`, `Sous-titres seuls (.srt)`. Transcodage désactivé (`OUTPUTS_NATIVE`) : `Vidéo (natif)`, `Audio WAV`, `Sous-titres seuls (.srt)`. Même logique qu'aujourd'hui dans `toggleTranscode`. |
| Sous-titres (.srt) | Interrupteur | Seulement si `profile.subtitles`. |
| Miniature | Interrupteur | Seulement si `profile.thumbnail`. |

**3. Destination**
- Chemin du dossier (ex. `~/Movies/Robloader`), tronqué au milieu s'il est long.
- Bouton « Choisir… » (commande existante `choose_destination`) et bouton « Afficher dans le Finder » (`reveal_in_folder`).

**4. Pied de volet** : informations d'état, reprises de l'actuelle ligne d'état.
- `cookies Chrome ✓` ou `cookies absents` (lien vers l'aide : `open_cookie_help`).
- `4K limitée (Deno absent)` seulement si `jsRuntime` est absent.
- **Mise à jour disponible** : l'actuelle bannière (`update-banner`) devient une carte en bas du volet, « Mise à jour disponible — vX.Y.Z » + « Installer et relancer ». Appliquer au passage les pistes 1 et 2 de `tauri/TODO.md` (spinner, message avant la fermeture).

### 4.4 Zone principale : file de téléchargements (HTML)

- **En-tête** : « File de téléchargements », compteur gris « N en cours » (masqué si 0), bouton « Nettoyer la liste » à droite (retire terminés, échecs et annulés).
- **Liste groupée** (carte à coins arrondis, lignes séparées par un filet). Chaque tâche :
  - vignette 16:9 à gauche (voir question ouverte §8 ; à défaut, l'icône du site) ;
  - titre sur une ligne, tronqué ;
  - statut en petit : `En attente` · `Analyse du lien…` · `Téléchargement 64 % · 12,3 Mo/s · reste 8 s` · `Transcodage 31 % · HEVC` · `Terminé · 1,2 Go` (vert) · `Échec : raison` (rouge) · `Annulé` ;
  - barre de progression fine pendant le téléchargement et le transcodage ;
  - bouton contextuel à droite : **Annuler** (en cours), **Afficher** (terminé, ouvre le Finder), **Réessayer** (échec), **Relancer** (annulé).
- **Clic droit sur une tâche** → **menu contextuel natif** : Afficher dans le Finder (grisé si pas terminé) · Copier le lien · Relancer · — · Retirer de la liste.
- **File vide** : « Aucun téléchargement pour l'instant. Colle un lien dans la barre d'outils. »
- La file **défile sous la barre d'outils**, avec l'effet de bord de défilement (flou dégressif sous la barre).
- Le branchement sur le moteur ne change pas : `start_download`, `cancel_download`, événement `task://update`.

### 4.5 Barre de menus (native, en français)

Reprendre le menu de `feat/presentation-liquid-glass` (`lib.rs`) **sans** le menu « Présentation » :
- **Robloader** : À propos, Services, Masquer, Masquer les autres, Tout afficher, Quitter.
- **Édition** : Annuler, Rétablir, Couper, Copier, Coller, Tout sélectionner. Obligatoire, sinon ⌘C/⌘V/⌘A cessent de marcher dans les champs.
- **Fenêtre** : Réduire, Plein écran, Fermer (⌘W).
- À envisager : « Fichier > Télécharger le lien (⌘↩) » et « Afficher dans le Finder ».

### 4.6 Apparence et système

- Clair / sombre : suit le système, comme aujourd'hui.
- Couleur d'accent : suit le système (`get_accent_color` existe déjà).
- **Curseur Liquid Glass de macOS 27** (Réglages > Apparence, clair ↔ teinté) : suivi automatiquement par la barre, les menus et le volet, puisque ce sont des matériaux natifs. **Rien à coder.**
- **Réduire la transparence** (Accessibilité) : géré par macOS sur les matériaux natifs.
- **macOS < 26** : pas de `NSGlassEffectView`. Repli : volet en vibrancy (`NSVisualEffectView`, matériau *sidebar*), barre d'outils classique.
- **Taille de fenêtre** : avec un volet de 290 px, le minimum actuel (560 × 420) est trop petit. Proposition : `minWidth` ≈ 780 et taille par défaut ≈ 1000 × 680, **sur macOS uniquement**.

---

## 5. Proposition de mise en œuvre technique

Contraintes du projet : Tauri v2, **une** fenêtre et **une** webview (WKWebView), `titleBarStyle: "Overlay"`, `hiddenTitle: true`, `transparent: true`, `macOSPrivateApi: true`, `window-vibrancy 0.6`, `objc2 0.6`. Pas de Mac sur la machine agent : tout se vérifie par build CI + test de Robin.

### 5.1 Barre d'outils native

**Option recommandée : une vraie `NSToolbar` créée en Rust.**
- Créer un délégué `NSToolbarDelegate` avec `objc2` (`define_class!`), fournissant les items : champ de lien (`NSToolbarItem` avec une vue `NSTextField` ou `NSSearchField` sans loupe, largeur flexible), espace flexible, bouton Télécharger.
- Attacher la barre à la `NSWindow` (`window.ns_window()`), `toolbarStyle = .unified`. Titre et sous-titre de fenêtre à la place du titre HTML (revoir `hiddenTitle`).
- Bouton proéminent teinté : vérifier l'API macOS 26 disponible (style d'item « prominent » ou teinte de fond) et prévoir un repli.
- **Pont avec le front** :
  - Rust → front : événements `toolbar://url-changed` (texte, à chaque frappe, pour la pastille et le profil), `toolbar://submit` (Entrée ou bouton), `toolbar://paste`.
  - Front → Rust : commande `toolbar_set_url(url)` (vider après ajout), `toolbar_set_source(label)` (pastille) et `toolbar_set_placeholder(text)`.
  - La détection du profil peut rester côté front (`SITE_PROFILES`) : le front reçoit le texte et renvoie pastille et texte indicatif.
- Le bouton Coller lit le presse-papiers : `NSPasteboard` côté Rust, ou plugin `clipboard-manager` de Tauri.
- Tout appel AppKit se fait sur le **thread principal** (`app.run_on_main_thread`).
- Côté CSS macOS : supprimer la barre de titre HTML (38 px) et laisser la place à la barre native (≈52 px). Le contenu passe dessous (fenêtre en `fullSizeContentView`, déjà le cas avec `Overlay`).

**Repli si la `NSToolbar` s'avère trop coûteuse** : barre en HTML qui imite la barre native, avec un `NSGlassEffectView` posé derrière (même mécanique que le volet, §5.2). Moins fidèle (pas de vrais contrôles natifs), mais bien plus simple.

### 5.2 Volet latéral en verre

Comme le volet ne recouvre pas la file (côte à côte, pas superposés), il n'a pas besoin de déformer le contenu de l'app. On peut donc garder **une seule webview** :
- Poser un `NSGlassEffectView` (coins ≈22 px) **derrière la webview transparente**, exactement sous le rectangle du volet.
- Le contenu du volet reste en HTML **à fond transparent** par-dessus.
- Le front mesure le volet (`ResizeObserver`) et envoie ses coordonnées à Rust via une commande `set_glass_regions([{x, y, w, h, radius}])`. Rust crée ou repositionne la vue de verre. Prévoir aussi le recalcul au redimensionnement de la fenêtre.
- **Reste de la fenêtre** : fond plein pour la file. Garder la vibrancy derrière l'ensemble, ou un fond opaque, à trancher au premier test visuel.
- Repli macOS < 26 : même rectangle en `NSVisualEffectView` (matériau sidebar).

Alternative écartée pour l'instant : `NSSplitViewController` avec un vrai volet latéral (macOS 26 le rend flottant en verre automatiquement). Il faudrait une **deuxième webview** pour le contenu du volet (multi-webview Tauri, fonction encore `unstable`), avec synchronisation d'état, focus et raccourcis entre les deux. Trop de risque pour le gain.

### 5.3 Front (React)

- Un layout dédié quand `data-os="macos"` : grille `[volet ≈312 px (marges comprises) | file]`. **Windows garde le layout actuel à l'identique.**
- Nouveaux composants : `Sidebar` (sections), `QualityGrid`, `Switch` (interrupteur style macOS), `GroupedList` / `GroupedRow`, `TaskCard` refondue (vignette + bouton contextuel).
- Sur macOS, la ligne lien, les lignes Extrait/Options, la ligne d'état et la bannière de mise à jour **quittent** la zone principale.
- **Menus natifs** via `@tauri-apps/api/menu` (`Menu`, `MenuItem`, `CheckMenuItem`, `menu.popup()`) : menu local « Format de sortie » et clic droit sur les tâches.

### 5.4 Rust

- Nouveau module macOS, par ex. `src-tauri/src/macos_chrome.rs` : barre d'outils + régions de verre, compilé seulement sur macOS (`#[cfg(target_os = "macos")]`).
- Nouvelles commandes : `set_glass_regions`, `toolbar_set_url`, `toolbar_set_source`, `toolbar_set_placeholder`.
- Dépendances probables : `objc2`, `objc2-foundation` et `objc2-app-kit` (API typées bien utiles pour `NSToolbar`), `objc2-core-foundation` **avec les features `["CFCGTypes", "objc2"]`** (pour `CGRect`).

---

## 6. Code réutilisable

Depuis la branche **`feat/presentation-liquid-glass`** (worktree `/home/robin/projects/Robloader-glass`) :
- `tauri/src-tauri/src/liquid_glass.rs` :
  - détection de `NSGlassEffectView` **au runtime** (`AnyClass::get(c"NSGlassEffectView")`), ce qui compile avec n'importe quel SDK et se replie proprement sur macOS < 26 ;
  - création d'une vue de verre (`initWithFrame`, `setAutoresizingMask`, insertion sous la webview avec `addSubview:positioned:relativeTo:`) ;
  - retrait propre (`isKindOfClass:` puis `removeFromSuperview`) et `clear_vibrancy` ;
  - persistance d'un réglage dans `app_config_dir()/presentation.json` (modèle pour d'autres préférences).
- `lib.rs` : construction du **menu macOS en français** (Robloader / Édition / Fenêtre), `apply_background` (fond de secours + `stabilize_content_on_resize` rejoué après ajout d'une vue).

À ne pas reprendre : le fond en verre plein cadre et le menu « Présentation > Liquid Glass » (remplacés par ce redesign). Le sort de cette branche est à confirmer avec Robin.

---

## 7. Étapes proposées

Chaque étape = une branche ou PR, un build CI `xcode-27` (workflow `build.yml`, `workflow_dispatch`), le DMG envoyé à Robin via `drop`, et son test sur Mac avant de passer à la suite.

1. **Layout macOS en HTML seul** : volet gauche + file, barre d'outils encore en HTML. Valide l'ergonomie et les tailles sans risque natif.
2. **Verre natif sous le volet** : `set_glass_regions` + réutilisation de `liquid_glass.rs`. Test du curseur d'intensité de macOS 27.
3. **Barre d'outils native** (`NSToolbar`) : champ de lien, Coller, Télécharger proéminent, titre et sous-titre. Retrait de la barre HTML sur macOS.
4. **Menus natifs** : format de sortie, clic droit des tâches, barre de menus en français.
5. **Finitions** : effet de bord de défilement, repli macOS < 26, tailles minimales, clavier (⌘L, Entrée), mise à jour in-app dans le volet (+ `TODO.md`).
6. **Vérification Windows** : aucune régression, le build Windows doit rester identique.

---

## 8. Arbitrages tranchés le 16/09/2026

| Question | Décision |
|---|---|
| Vignettes dans la file | **La vraie miniature**, récupérée à l'analyse du lien, mise en cache sur le disque, repli sur l'icône du site quand il n'y en a pas. |
| Profils sans échelle de qualité (TikTok, Instagram, X, Weibo) | **Section Qualité masquée**, comme le fait déjà `main`. Pas de ligne « qualité automatique ». |
| Bannière de mise à jour | **Bouton dans la barre d'outils** (pastille orange + libellé), pas de carte dans le volet. |
| Titre dans la barre | **« Robloader 2.1.7 » sur une seule ligne**, sans l'état des cookies, qui reste en pied de volet. |
| Branche `feat/presentation-liquid-glass` | Gardée pour l'instant, décision reportée. |
| Fond derrière la file | **Fond plein** (pas de translucide sous la file). |
| Affichage de la progression | **Anneau** près du bouton d'action, pas de barre : bleu d'accent pour le téléchargement, **violet** pour le transcodage. |

### Exigences ajoutées le 16/09/2026

1. **Le plus de natif possible.** Tout contrôle qui existe en AppKit doit être natif : barre d'outils et ses items, interrupteurs (`NSSwitch`), menus locaux (`NSPopUpButton`), menus contextuels, barre de menus. Le HTML ne garde que la file de téléchargements et le contenu du volet. Le repli HTML de la §5.1 devient un dernier recours, pas une option de confort.
2. **Libellés du volet, courts.** « Extraire un passage » et « Transcodage » **sans sous-texte**. L'interrupteur est **toujours sur la même ligne que son libellé**, jamais renvoyé à la ligne (c'était le défaut de la maquette du 15).
3. **Révélations animées.** Activer « Extraire un passage » fait apparaître la ligne Début/Fin ; « Format de sortie » **n'existe que si Transcodage est activé**. Les deux apparaissent et disparaissent avec une animation fluide (en natif `NSAnimationContext` / `NSStackView` animé, en HTML une transition d'environ 0,3 s).
4. **Interrupteurs natifs.** `NSSwitch` avec la couleur d'accent du système et le matériau Liquid Glass de macOS 26/27. Rien de dessiné en CSS.
5. **File de téléchargements : hauteur de ligne constante et texte centré.** Le titre et l'état sont centrés verticalement sur la vignette. La progression est dessinée **hors du flux** (superposée à la ligne), donc elle ne prend aucune hauteur et une ligne ne saute jamais en changeant d'état — c'était le défaut de la première tentative, où la barre occupait une troisième rangée et poussait le texte vers le haut.
6. **Couleurs de progression.** Bleu d'accent pour le téléchargement, **violet (`systemPurple`)** pour le transcodage, afin de distinguer les deux phases d'un coup d'œil.

Maquette de référence, verrouillée sur ces décisions : `docs/redesign-macos/maquette-d3-arbitrages.html`. **Plus aucune question ouverte** : l'étape 1 du §7 peut démarrer.

---

## 9. Pièges déjà rencontrés (à ne pas redécouvrir)

- **SDK de liaison** : le look natif macOS 27 exige un binaire lié au SDK 27 (runner `xcode-27`). Vérifiable dans le log CI (`vtool -show-build` → `sdk 27.0`).
- **`cargo check --target aarch64-apple-darwin` échoue sur la machine Linux** (la dépendance `objc2-exception-helper` exige le compilateur C d'Apple). La seule vérification possible est le build CI macOS.
- **Signature ad hoc obligatoire** sur Apple Silicon (`"signingIdentity": "-"`), sinon « app endommagée ».
- **Remplacer le menu par défaut de Tauri** supprime aussi le menu Édition, et donc ⌘C/⌘V/⌘A dans les champs : toujours le reconstruire.
- **AppKit uniquement sur le thread principal** : `apply_vibrancy` renvoie une erreur hors thread principal. Utiliser `run_on_main_thread` pour tout ce qui vient d'un événement.
- **`stabilize_content_on_resize`** (`lib.rs`) doit être rejoué après l'ajout d'une nouvelle vue native, sinon le contenu « saute » pendant l'animation de zoom.
- **`objc2-core-foundation`** : `CGRect` exige la feature `CFCGTypes` (pas `CGGeometry`), et son encodage pour `msg_send!` exige la feature `objc2`.
- **Verre plein cadre = illisible** (POC `feat/liquid-glass`) : ne jamais mettre de verre derrière du contenu à lire.
