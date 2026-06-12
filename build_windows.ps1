# Build de Robloader pour Windows (dossier dist\Robloader\Robloader.exe, + installeur si Inno Setup present).
# Usage : powershell -ExecutionPolicy Bypass -File build_windows.ps1
$ErrorActionPreference = "Stop"
Set-Location -Path $PSScriptRoot

Write-Host "================ Robloader - build Windows ================"

# 1) Dependances Python
python -m pip install --upgrade pip | Out-Null
python -m pip install --upgrade pyinstaller customtkinter yt-dlp certifi pywinstyles | Out-Null

New-Item -ItemType Directory -Force -Path bin | Out-Null

# 2) Deno (moteur JS pour le nsig / 4K)
if (-not (Test-Path bin\deno.exe)) {
    Write-Host "-> Deno..."
    Invoke-WebRequest -Uri "https://github.com/denoland/deno/releases/latest/download/deno-x86_64-pc-windows-msvc.zip" -OutFile "$env:TEMP\deno.zip"
    Expand-Archive -Force "$env:TEMP\deno.zip" -DestinationPath bin
    Remove-Item "$env:TEMP\deno.zip"
}

# 3) ffmpeg + ffprobe statiques (BtbN, autonomes)
if ((-not (Test-Path bin\ffmpeg.exe)) -or (-not (Test-Path bin\ffprobe.exe))) {
    Write-Host "-> ffmpeg/ffprobe statiques..."
    # NB : BtbN tague sa release "latest" en PRE-RELEASE -> le mot magique releases/latest/download
    # renvoie 404 (GitHub ne resout pas une pre-release). On vise le tag litteral : releases/download/latest.
    Invoke-WebRequest -Uri "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-win64-gpl.zip" -OutFile "$env:TEMP\ff.zip"
    Expand-Archive -Force "$env:TEMP\ff.zip" -DestinationPath "$env:TEMP\ff"
    $ffbin = Get-ChildItem -Path "$env:TEMP\ff" -Recurse -Filter ffmpeg.exe | Select-Object -First 1
    Copy-Item $ffbin.FullName bin\ffmpeg.exe -Force
    Copy-Item (Join-Path $ffbin.DirectoryName "ffprobe.exe") bin\ffprobe.exe -Force
    Remove-Item "$env:TEMP\ff.zip"; Remove-Item -Recurse -Force "$env:TEMP\ff"
}

# 4) Build PyInstaller
Write-Host "-> PyInstaller..."
Remove-Item -Recurse -Force build, dist -ErrorAction SilentlyContinue
pyinstaller --noconfirm Robloader.spec

# 5) Installeur Inno Setup (si ISCC est installe)
$iscc = Get-Command iscc.exe -ErrorAction SilentlyContinue
if ($iscc) {
    Write-Host "-> Installeur Inno Setup..."
    & $iscc.Source installer\Robloader.iss
} else {
    Write-Host "  (Inno Setup absent : pas d'installeur. dist\Robloader\Robloader.exe est utilisable tel quel.)"
}

Write-Host ""
Write-Host "================ Termine ================"
Write-Host "App : dist\Robloader\Robloader.exe"
Write-Host "Installeur (si genere) : installer\Output\Robloader-Setup.exe"
