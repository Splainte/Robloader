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

# Mode UNIVERSAL2 (CI) : on produit un binaire qui tourne NATIVEMENT sur Intel ET Apple Silicon.
# Les runners Intel GitHub (macos-13) etant en cours de retrait, on ne peut plus builder Intel
# nativement -> on fusionne les DEUX arches (arm64 + x86_64) avec lipo sur le runner arm64, et
# PyInstaller emballe en universal2 (cf ROBLOADER_TARGET_ARCH dans Robloader.spec). Requiert un
# Python universal2 (installe depuis python.org en CI).
UNIVERSAL=""
if [ "${ROBLOADER_TARGET_ARCH:-}" = "universal2" ]; then UNIVERSAL=1; echo "Mode         : UNIVERSAL2 (Intel + Apple Silicon)"; fi

# Fusionne deux binaires "thin" (arm64 + x86_64) en un binaire universel.
make_universal () {  # $1=sortie  $2=bin_arm64  $3=bin_x86_64
  lipo -create "$2" "$3" -output "$1" && chmod +x "$1"
  echo "   lipo -> $1 : $(lipo -archs "$1" 2>/dev/null)"
}

# --- Deno (moteur JS pour le nsig / 4K) ---
fetch_deno () {  # $1=target_triplet  $2=sortie
  curl -fsSL -o "/tmp/deno_$1.zip" \
    "https://github.com/denoland/deno/releases/latest/download/deno-$1.zip"
  rm -rf "/tmp/deno_$1.d"; mkdir -p "/tmp/deno_$1.d"
  unzip -oq "/tmp/deno_$1.zip" -d "/tmp/deno_$1.d"
  mv "/tmp/deno_$1.d/deno" "$2"; rm -rf "/tmp/deno_$1.d" "/tmp/deno_$1.zip"
}
if [ ! -x bin/deno ]; then
  if [ -n "$UNIVERSAL" ]; then
    echo "-> Deno universel (arm64 + x86_64)..."
    fetch_deno aarch64-apple-darwin /tmp/deno.arm64
    fetch_deno x86_64-apple-darwin  /tmp/deno.x86
    make_universal bin/deno /tmp/deno.arm64 /tmp/deno.x86; rm -f /tmp/deno.arm64 /tmp/deno.x86
  else
    echo "-> Deno ($DENO_TARGET)..."; fetch_deno "$DENO_TARGET" bin/deno
  fi
else echo "-> bin/deno deja present."; fi

# --- ffmpeg + ffprobe STATIQUES (autonomes -> tournent sur n'importe quel Mac) ---
fetch_ff () {  # $1=ffmpeg|ffprobe  $2=arm64|amd64  $3=sortie
  curl -fsSL -o "/tmp/$1_$2.zip" \
    "https://ffmpeg.martin-riedl.de/redirect/latest/macos/$2/release/$1.zip"
  rm -rf "/tmp/$1_$2.d"; mkdir -p "/tmp/$1_$2.d"
  unzip -oq "/tmp/$1_$2.zip" -d "/tmp/$1_$2.d"
  mv "/tmp/$1_$2.d/$1" "$3"; rm -rf "/tmp/$1_$2.d" "/tmp/$1_$2.zip"
}
fetch_ff_universal () {  # $1=ffmpeg|ffprobe
  echo "-> $1 universel (arm64 + amd64)..."
  fetch_ff "$1" arm64 "/tmp/$1.arm64"
  fetch_ff "$1" amd64 "/tmp/$1.x86"
  make_universal "bin/$1" "/tmp/$1.arm64" "/tmp/$1.x86"; rm -f "/tmp/$1.arm64" "/tmp/$1.x86"
}
if [ -n "$UNIVERSAL" ]; then
  [ -x bin/ffmpeg ]  || fetch_ff_universal ffmpeg  || true
  [ -x bin/ffprobe ] || fetch_ff_universal ffprobe || true
else
  echo "-> ffmpeg/ffprobe statiques ($FF_ARCH)..."
  [ -x bin/ffmpeg ]  || fetch_ff ffmpeg  "$FF_ARCH" bin/ffmpeg  || true
  [ -x bin/ffprobe ] || fetch_ff ffprobe "$FF_ARCH" bin/ffprobe || true
fi
# Repli Homebrew si le telechargement statique a echoue (build LOCAL uniquement : dylib -> ne tourne
# PAS sur un autre Mac, et single-arch -> incompatible avec un build universel a distribuer).
if [ ! -x bin/ffmpeg ] || [ ! -x bin/ffprobe ]; then
  if [ -n "$UNIVERSAL" ]; then
    echo "ERREUR : telechargement ffmpeg/ffprobe universel echoue (pas de repli en mode universal2)."; exit 1
  elif command -v brew >/dev/null 2>&1 && [ -x "$(brew --prefix)/bin/ffmpeg" ]; then
    echo "  ! Repli Homebrew (dependances dylib -> ne tourne PAS sur un autre Mac)."
    cp "$(brew --prefix)/bin/ffmpeg" "$(brew --prefix)/bin/ffprobe" bin/ 2>/dev/null || true
  else
    echo "ERREUR : ffmpeg/ffprobe introuvables. 'brew install ffmpeg' puis relancez."; exit 1
  fi
fi

chmod +x bin/* 2>/dev/null || true
xattr -dr com.apple.quarantine bin/* 2>/dev/null || true

# Verification : en universal2, l'app et tous les binaires embarques doivent etre 2-arch.
if [ -n "$UNIVERSAL" ]; then
  echo "-> Verification des arches embarquees :"
  for b in bin/deno bin/ffmpeg bin/ffprobe; do
    [ -e "$b" ] && echo "   $b : $(lipo -archs "$b" 2>/dev/null || echo '?')"
  done
fi

# --- Build ---
echo "-> PyInstaller..."
rm -rf build dist
pyinstaller --noconfirm Robloader.spec
xattr -dr com.apple.quarantine "dist/Robloader.app" 2>/dev/null || true
if [ -n "$UNIVERSAL" ]; then
  APP_BIN="dist/Robloader.app/Contents/MacOS/Robloader"
  echo "-> Arche du binaire app : $(lipo -archs "$APP_BIN" 2>/dev/null || echo '?')"
  if ! lipo -archs "$APP_BIN" 2>/dev/null | grep -q x86_64 || ! lipo -archs "$APP_BIN" 2>/dev/null | grep -q arm64; then
    echo "ERREUR : le binaire app n'est PAS universal2 (Python universal2 manquant ?)."; exit 1
  fi
fi

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
