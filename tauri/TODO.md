# TODO

## MàJ silencieuse : rendre le passage plus visuel (2026-07-10)

Actuellement (v2.1.1+) : clic sur « Installer et relancer » → texte statique
« Téléchargement… » → la fenêtre se ferme net (`app.exit(0)`) pendant que le
script détaché (NSIS `/S` puis relance) tourne en silence → l'app réapparaît.
Le trou sans fenêtre pendant l'install donne l'impression de crash.

Pistes retenues (cumulables, cf. `engine.rs` `install_update` + `App.tsx`) :

1. **Spinner pendant le téléchargement** — le bouton dit déjà « Téléchargement… »
   mais reste statique. Ajouter un spinner CSS indéterminé à côté (pas besoin
   d'un vrai %, le fichier est petit et curl finit en quelques secondes).
2. **Prévenir juste avant le trou noir** (le plus payant) — juste avant
   `app.exit(0)`, remplacer le texte par un message explicite genre « L'app va
   se fermer quelques secondes puis se relancer automatiquement ». Nécessite un
   petit event Rust→front (ex. `update://installing`) émis avant de spawn le
   script d'install, pour laisser un dernier message rassurant à l'écran.
3. **Confirmation au retour** (bonus) — au relancement, toast discret
   « Mise à jour effectuée — vX.Y.Z » en comparant la version stockée avant le
   clic (localStorage) à `getVersion()` actuelle.

Priorité : 1 + 2 ensemble (petit diff, couvre l'essentiel du problème perçu).
Le 3 est optionnel.

## S'adapter à la couleur d'accentuation du système (2026-07-11)

Robloader devrait reprendre la couleur d'accentuation choisie par l'utilisateur
dans les réglages système (macOS ET Windows), au lieu d'une couleur fixe
codée en dur dans App.css. Actuellement KO sur Windows 11 (à vérifier aussi
côté macOS, pas confirmé fonctionnel).

Pistes à creuser :
- **Windows** : `DwmGetColorizationColor` (API Win32) ou lecture du registre
  `HKEY_CURRENT_USER\Software\Microsoft\Windows\DWM\AccentColor` côté Rust,
  puis transmission au front (event Tauri) pour piloter une CSS var.
- **macOS** : `NSColor.controlAccentColor` (AppKit) côté Rust/Swift, même
  principe de transmission vers le front.
- Prévoir un fallback (couleur actuelle par défaut) si la lecture échoue ou
  sur les OS/versions non supportés.
