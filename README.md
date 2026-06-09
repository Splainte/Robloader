# ⚙️ Robloader

**Télécharge des vidéos depuis YouTube, TikTok, Instagram, Twitter (X) et Weibo — en un clic.**

Colle une URL : Robloader détecte le site, télécharge la vidéo et te rend un fichier prêt à l'emploi.

---

## 📥 Installation

Pas de configuration, pas de dépendances à installer. Télécharge la dernière version pour ton système :

### → [Télécharger la dernière version](https://github.com/Splainte/Robloader/releases/latest)

- **Windows** : lance `Robloader-Setup-Windows.exe` et suis l'installeur.

  *(À l'installation il faudra forcer windows à ouvrir l'installeur en cliquand sur "en savoir plus" puis "éxecuter quand même", car l'app n'est pas signée.)*
- **macOS** : ouvre `Robloader-Setup-macOS.dmg` et glisse l'app dans **Applications**.
  Compatible Apple Silicon et Intel !

  *(Au premier lancement il faudra forcer l'ouverture de l'app en passant par le menu sécurité des réglages système, car l'app n'est pas signée.)*

Tout le nécessaire est déjà inclus dans l'app.

---

## ✨ Fonctionnalités

- **Multi-plateformes** — YouTube, TikTok, Instagram, X (Twitter) et Weibo. Colle l'URL, clique, c'est parti.
- **Interface qui s'adapte à la source** — l'app détecte le site collé et ajuste **en fondu** les couleurs des boutons et les options disponibles à sa charte (rouge YouTube, magenta Instagram…).
- **Extraction d'un segment** — récupère seulement un passage de la vidéo grâce à un timecode de début et de fin, ou la vidéo entière.
- **Choix de la qualité** (YouTube) — Max (jusqu'à 4K), 1440p, 1080p, 720p, 480p. Ton choix est mémorisé.
- **Prêt pour Premiere Pro** — sortie HEVC (H.265) optimisée pour le meilleur combo qualité/poids, ou conversion en ProRes pour un montage fluide.
- **Encodage accéléré par le GPU** — utilise la carte graphique (NVIDIA / Apple Silicon) quand c'est possible, avec repli automatique sur le processeur.
- **File d'attente** — Permet d'enchaîner plusieurs téléchargements, chacun annulable à tout moment.
- **Cookies automatiques** — se connecte tout seul à ton compte via le navigateur pour débloquer la 4K, les vidéos restreintes ou les contenus privés (Instagram, X) ; un bouton **« Réparer »** t'explique quoi faire si besoin.
- **Mises à jour automatiques** — t'avertit quand une nouvelle version est disponible et l'installe en quelques clics.
- **Windows & macOS**.

---

## ▶️ Utilisation

1. Colle l'**URL** de la vidéo (YouTube, TikTok, Instagram, X, Weibo).
2. *(Optionnel)* indique un **Début** et une **Fin** pour ne récupérer qu'un extrait.
3. Choisis la **destination** (par défaut : ton dossier Téléchargements).
4. Clique sur **Télécharger** — ton fichier est prêt. 🎬

---

## ❓ Un souci ?

La plupart des problèmes (vidéo qui demande une connexion, contenu privé, 4K qui ne passe pas) se règlent en étant **connecté au site dans ton navigateur** : Robloader utilise alors automatiquement ta session. Le bouton **« Réparer »** dans l'app explique la marche à suivre.

> 🧑‍💻 Tu veux construire l'app depuis les sources ou contribuer ? Vois la documentation technique dans le code (`Robloader.py`) et les scripts de build (`build_windows.ps1`, `build_macos.sh`).
