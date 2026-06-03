#!/usr/bin/env bash
# Build de Robloader.app (+ DMG) pour macOS.
# Usage : ./build_macos.sh
# A lancer SUR un Mac (PyInstaller ne cross-compile pas). Produit dist/Robloader.app et dist/Robloader.dmg.
#
# Prerequis : un Python avec Tk 8.6+/9.0 (PAS le Python systeme en Tk 8.5 -> UI grise).
#   brew install python-tk
#   "$(brew --prefix)/bin/python3" -m venv .venv && source .venv/bin/activate
set -euo pipefail
cd "$(dirname "$0")"

echo "================ Robloader — build macOS ================"

ARCH="$(uname -m)"   # arm64 (Apple Silicon) ou x86_64 (Intel)
case "$ARCH" in
  arm64)  DENO_TARGET="aarch64-apple-darwin"; FF_ARCH="arm64" ;;
  x86_64) DENO_TARGET="x86_64-apple-darwin";  FF_ARCH="amd64" ;;
  *) echo "Architecture non geree : $ARCH" ; exit 1 ;;
esac
echo "Architecture : $ARCH"

# Verifie qu'on n'est pas sur le Tk systeme 8.5 (sinon l'UI sera grise)
TKV="$(python3 -c 'import tkinter; print(tkinter.TkVersion)' 2>/dev/null || echo '?')"
echo "Tk version   : $TKV"
case "$TKV" in
  8.6|9.0|9.*) : ;;
  *) echo "  ⚠ Tk $TKV : risque d'UI grise. Recreez le venv avec un Python Homebrew (brew install python-tk)." ;;
esac

echo "-> Dependances Python..."
python3 -m pip install --upgrade pip >/dev/null
python3 -m pip install --upgrade pyinstaller customtkinter "yt-dlp" >/dev/null

mkdir -p bin

# --- Deno (moteur JS pour le nsig / 4K) ---
if [ ! -x bin/deno ]; then
  echo "-> Deno ($DENO_TARGET)..."
  curl -fsSL -o /tmp/deno.zip \
    "https://github.com/denoland/deno/releases/latest/download/deno-${DENO_TARGET}.zip"
  unzip -oq /tmp/deno.zip -d bin && rm -f /tmp/deno.zip
else echo "-> bin/deno deja present."; fi

# --- ffmpeg + ffprobe STATIQUES (autonomes -> tournent sur n'importe quel Mac) ---
fetch_static () {
  local name="$1"
  echo "-> $name statique ($FF_ARCH)..."
  curl -fsSL -o "/tmp/$name.zip" \
    "https://ffmpeg.martin-riedl.de/redirect/latest/macos/$FF_ARCH/release/$name.zip"
  unzip -oq "/tmp/$name.zip" -d bin && rm -f "/tmp/$name.zip"
}
if [ ! -x bin/ffmpeg ];  then fetch_static ffmpeg  || true; fi
if [ ! -x bin/ffprobe ]; then fetch_static ffprobe || true; fi
# Repli Homebrew si le telechargement statique a echoue (OK pour tester ce Mac, pas pour distribuer)
if [ ! -x bin/ffmpeg ] || [ ! -x bin/ffprobe ]; then
  if command -v brew >/dev/null 2>&1 && [ -x "$(brew --prefix)/bin/ffmpeg" ]; then
    echo "  ! Repli Homebrew (dependances dylib -> ne tourne PAS sur un autre Mac)."
    cp "$(brew --prefix)/bin/ffmpeg" "$(brew --prefix)/bin/ffprobe" bin/ 2>/dev/null || true
  else
    echo "ERREUR : ffmpeg/ffprobe introuvables. 'brew install ffmpeg' puis relancez."; exit 1
  fi
fi

chmod +x bin/* 2>/dev/null || true
xattr -dr com.apple.quarantine bin/* 2>/dev/null || true

# --- Build ---
echo "-> PyInstaller..."
rm -rf build dist
pyinstaller --noconfirm Robloader.spec
xattr -dr com.apple.quarantine "dist/Robloader.app" 2>/dev/null || true

# --- DMG (glisser-deposer dans Applications) ---
echo "-> Creation du DMG..."
STAGE="$(mktemp -d)"
cp -R "dist/Robloader.app" "$STAGE/"
ln -s /Applications "$STAGE/Applications"
rm -f "dist/Robloader.dmg"
hdiutil create -volname "Robloader" -srcfolder "$STAGE" -ov -format UDZO "dist/Robloader.dmg" >/dev/null
rm -rf "$STAGE"
xattr -dr com.apple.quarantine "dist/Robloader.dmg" 2>/dev/null || true

echo
echo "================ Termine ================"
echo "App : dist/Robloader.app"
echo "DMG : dist/Robloader.dmg   (a distribuer : ouvrir, glisser l'app dans Applications)"
echo
echo "4K : marche si l'utilisateur est connecte a YouTube dans Chrome/Safari (cookies auto),"
echo "     ou en posant un cookies.txt a cote de l'app."
echo "1er lancement (app non signee) : clic droit sur l'app > Ouvrir."
