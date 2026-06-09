# ⚙️ Robloader

**Télécharge des vidéos YouTube et les prépare pour Adobe Premiere Pro, en un clic.**

Colle une URL, choisis ta qualité, et récupère un fichier `.mp4` prêt à importer dans Premiere — sans réglages, sans ligne de commande.

---

## 📥 Installation

Pas de configuration, pas de dépendances à installer. Télécharge la dernière version pour ton système :

### → [Télécharger la dernière version](https://github.com/Splainte/Robloader/releases/latest)

- **Windows** : lance `Robloader-Setup.exe` et suis l'installeur.
- **macOS** : ouvre `Robloader.dmg` et glisse l'app dans **Applications**.
  *(Au premier lancement : clic droit ▸ Ouvrir, car l'app n'est pas signée.)*

Tout le nécessaire est déjà inclus dans l'app.

---

## ✨ Fonctionnalités

- **Téléchargement YouTube simple** — colle l'URL, clique, c'est parti.
- **Extraction d'un segment** — récupère seulement un passage de la vidéo grâce à un timecode de début et de fin, ou la vidéo entière.
- **Choix de la qualité** — Max (jusqu'à 4K), 1440p, 1080p, 720p, 480p. Ton choix est mémorisé.
- **Prêt pour Premiere Pro** — sortie HEVC (H.265) optimisée pour un import fluide, ou conversion en ProRes pour le montage.
- **Encodage accéléré par le GPU** — utilise la carte graphique (NVIDIA / Apple) quand c'est possible, avec repli automatique sur le processeur.
- **File d'attente** — enchaîne plusieurs téléchargements, chacun annulable à tout moment.
- **Cookies automatiques** — se connecte tout seul à ton compte YouTube via le navigateur pour débloquer la 4K et les vidéos restreintes ; un bouton **« Réparer »** t'explique quoi faire si besoin.
- **Mises à jour automatiques** — t'avertit quand une nouvelle version est disponible et l'installe en un clic.
- **Windows & macOS**.

---

## ▶️ Utilisation

1. Colle l'**URL** de la vidéo YouTube.
2. *(Optionnel)* indique un **Début** et une **Fin** pour ne récupérer qu'un extrait.
3. Choisis la **destination** (par défaut : ton dossier Téléchargements).
4. Clique sur **Télécharger** — ton fichier `.mp4` est prêt pour Premiere Pro. 🎬

---

## ❓ Un souci ?

La plupart des problèmes (vidéo qui demande une connexion, 4K qui ne passe pas) se règlent en étant **connecté à YouTube dans ton navigateur** : Robloader utilise alors automatiquement ta session. Le bouton **« Réparer »** dans l'app explique la marche à suivre.

> 🧑‍💻 Tu veux construire l'app depuis les sources ou contribuer ? Vois la documentation technique dans le code (`Robloader.py`) et les scripts de build (`build_windows.ps1`, `build_macos.sh`).
