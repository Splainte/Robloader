#!/usr/bin/env bash
# Build de Robloader.app pour macOS.
# Usage : ./build_macos.sh
# Produit dist/Robloader.app . A lancer SUR un Mac (PyInstaller ne cross-compile pas).
set -euo pipefail
cd "$(dirname "$0")"

echo "================ Robloader — build macOS ================"

ARCH="$(uname -m)"   # arm64 (Apple Silicon) ou x86_64 (Intel)
case "$ARCH" in
  arm64)  DENO_TARGET="aarch64-apple-darwin" ;;
  x86_64) DENO_TARGET="x86_64-apple-darwin" ;;
  *) echo "Architecture non geree : $ARCH" ; exit 1 ;;
esac
echo "Architecture detectee : $ARCH"

# 1) Dependances Python (dans l'environnement courant — pensez a un venv)
echo "-> Installation des dependances Python..."
python3 -m pip install --upgrade pip >/dev/null
python3 -m pip install --upgrade pyinstaller customtkinter "yt-dlp" >/dev/null

mkdir -p bin

# 2) Deno (URL de release stable) — moteur JS requis pour le nsig (4K)
if [ ! -x bin/deno ]; then
  echo "-> Telechargement de Deno ($DENO_TARGET)..."
  curl -fsSL -o /tmp/deno.zip \
    "https://github.com/denoland/deno/releases/latest/download/deno-${DENO_TARGET}.zip"
  unzip -oq /tmp/deno.zip -d bin
  rm -f /tmp/deno.zip
else
  echo "-> bin/deno deja present."
fi

# 3) ffmpeg + ffprobe
if [ ! -x bin/ffmpeg ] || [ ! -x bin/ffprobe ]; then
  if command -v brew >/dev/null 2>&1 && [ -x "$(brew --prefix)/bin/ffmpeg" ]; then
    echo "-> Copie de ffmpeg/ffprobe depuis Homebrew (OK pour tester sur CE Mac)."
    cp "$(brew --prefix)/bin/ffmpeg"  bin/
    cp "$(brew --prefix)/bin/ffprobe" bin/
    echo "   ! Binaires Homebrew = dependances dylib -> ne tourneront PAS sur un autre Mac."
    echo "   ! Pour DISTRIBUER : remplacez bin/ffmpeg et bin/ffprobe par des builds STATIQUES (cf README)."
  else
    echo "ERREUR : bin/ffmpeg et bin/ffprobe manquants, et Homebrew introuvable."
    echo "  -> 'brew install ffmpeg' puis relancez, OU posez des binaires statiques dans ./bin/"
    exit 1
  fi
else
  echo "-> bin/ffmpeg et bin/ffprobe deja presents."
fi

chmod +x bin/* 2>/dev/null || true
xattr -dr com.apple.quarantine bin/* 2>/dev/null || true

# 4) Build PyInstaller
echo "-> Build PyInstaller..."
rm -rf build dist
pyinstaller --noconfirm Robloader.spec

# 5) Lever la quarantaine sur le .app produit
xattr -dr com.apple.quarantine "dist/Robloader.app" 2>/dev/null || true

echo
echo "================ Termine ================"
echo "App : dist/Robloader.app"
echo "Lancer : open dist/Robloader.app"
echo
echo "Pour la 4K, poser a cote de l'app (ou dans ~/Library/Application Support/Robloader/) :"
echo "  - cookies.txt  (export YouTube connecte)"
echo "Deno et ffmpeg/ffprobe sont deja embarques dans l'app."
echo
echo "Si macOS bloque l'ouverture (app non signee) : clic droit > Ouvrir, ou"
echo "  xattr -dr com.apple.quarantine dist/Robloader.app"
