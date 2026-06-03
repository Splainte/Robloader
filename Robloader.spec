# -*- mode: python ; coding: utf-8 -*-
# PyInstaller spec — Robloader (.app macOS)
# Build : ./build_macos.sh   (qui remplit bin/ puis lance : pyinstaller Robloader.spec)
import os
from PyInstaller.utils.hooks import collect_data_files

# Binaires embarques (deposes dans bin/ par build_macos.sh). 'dst="."' -> racine du bundle,
# que le code retrouve via la resolution robuste de ffmpeg + l'injection PATH.
_wanted = [('bin/ffmpeg', '.'), ('bin/ffprobe', '.'), ('bin/deno', '.')]
binaries = [(src, dst) for (src, dst) in _wanted if os.path.exists(src)]

# customtkinter embarque des assets (themes, polices) qu'il faut copier.
datas = collect_data_files('customtkinter')

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
    target_arch=None,         # arch courante (arm64 ou x86_64)
)

coll = COLLECT(
    exe,
    a.binaries,
    a.datas,
    strip=False,
    upx=False,
    name='Robloader',
)

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
