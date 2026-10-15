# TODO

## Passer au module de mise à jour officiel de Tauri (2026-10-15)

Aujourd'hui la MàJ est maison (`check_update` / `install_update` dans
`engine.rs`) : lecture de l'API GitHub Releases puis téléchargement de
l'installeur par curl, **sans vérification de signature**. Si le compte GitHub
ou une release était compromis, l'app installerait n'importe quel binaire.

Copiteur est passé au plugin officiel `tauri-plugin-updater` (v1.0.0, PR #5
de Splainte/Copiteur) — modèle à reprendre tel quel :

- Paire de clés `npx tauri signer generate` ; clé publique dans
  `tauri.conf.json` (`plugins.updater.pubkey`), clé privée + mot de passe en
  secrets du dépôt (`TAURI_SIGNING_PRIVATE_KEY[_PASSWORD]`). **La clé privée
  ne doit jamais être perdue** (sinon plus de MàJ possible).
- `bundle.createUpdaterArtifacts: true` ; endpoint
  `https://github.com/Splainte/Robloader/releases/latest/download/latest.json` ;
  Windows `installMode: "passive"`.
- Workflow `release.yml` de Copiteur : release brouillon (notes = message du
  tag annoté), builds signés, puis UN job final qui écrit `latest.json`
  (évite que les builds Windows/macOS l'écrasent en parallèle).
- Front : `check()` / `update.download()` / `update.install()` / `relaunch()`
  (`Updater.tsx` de Copiteur), installation bloquée pendant un téléchargement.
- Transition : les versions actuelles ne connaissent que l'ancien système. La
  première version « plugin » doit donc encore être installée par l'ancien
  mécanisme (garder l'asset que lit `check_update`) ; les suivantes passeront
  par le plugin.
- Rend caduc le point ci-dessous (le plugin gère progression + relance).

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
