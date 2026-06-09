# -*- mode: python ; coding: utf-8 -*-
# PyInstaller spec — Robloader (cross-platform : .app macOS / dossier .exe Windows)
# Build : ./build_macos.sh (mac)  ou  build_windows.ps1 (Windows). Remplissent bin/ puis lancent
# pyinstaller Robloader.spec.
import os
import sys
from PyInstaller.utils.hooks import collect_data_files

IS_MAC = sys.platform == 'darwin'
IS_WIN = sys.platform.startswith('win')

# Binaires embarques : tout ce que le script de build a depose dans bin/ (ffmpeg, ffprobe, deno,
# avec ou sans .exe). 'dst="."' -> racine ; le code les retrouve via resolution robuste + PATH.
binaries = []
if os.path.isdir('bin'):
    binaries = [(os.path.join('bin', f), '.') for f in os.listdir('bin')]

# customtkinter embarque des assets (themes, polices) + nos icones.
_icons = [(f, '.') for f in ('logo.png', 'logo.icns', 'logo.ico') if os.path.exists(f)]
datas = collect_data_files('customtkinter') + collect_data_files('certifi') + _icons

_exe_icon = 'logo.icns' if (IS_MAC and os.path.exists('logo.icns')) else \
            ('logo.ico' if os.path.exists('logo.ico') else None)

a = Analysis(
    ['Robloader.py'],
    pathex=[],
    binaries=binaries,
    datas=datas,
    hiddenimports=['customtkinter'],
    hookspath=[],
    hooksconfig={},
    runtime_hooks=[],
    excludes=[],
    noarchive=False,
)
pyz = PYZ(a.pure)

exe = EXE(
    pyz,
    a.scripts,
    [],
    exclude_binaries=True,
    name='Robloader',
    debug=False,
    bootloader_ignore_signals=False,
    strip=False,
    upx=False,
    console=False,            # app fenetree (pas de terminal)
    argv_emulation=False,     # desactive (source de soucis de demarrage, inutile ici)
    # arch cible : None = arch courante (build local). En CI macOS on exporte
    # ROBLOADER_TARGET_ARCH=universal2 -> un binaire universel Intel + Apple Silicon
    # (requiert un Python universal2 ET des bin/ universels, cf build_macos.sh).
    target_arch=(os.environ.get('ROBLOADER_TARGET_ARCH') or None),
    icon=_exe_icon,
)

coll = COLLECT(
    exe,
    a.binaries,
    a.datas,
    strip=False,
    upx=False,
    name='Robloader',
)

# macOS : on emballe en .app. Windows/Linux : le dossier dist/Robloader/ contient Robloader(.exe).
if IS_MAC:
    app = BUNDLE(
        coll,
        name='Robloader.app',
        icon='logo.icns' if os.path.exists('logo.icns') else None,
        bundle_identifier='fr.humanoid.robloader',
        info_plist={
            'NSHighResolutionCapable': True,
            'LSMinimumSystemVersion': '11.0',
            'CFBundleShortVersionString': '1.0.0',
        },
    )
