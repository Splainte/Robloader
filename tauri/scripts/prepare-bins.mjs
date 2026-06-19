/**
 * Télécharge / copie yt-dlp, ffmpeg, ffprobe dans src-tauri/binaries/
 * nommés avec le triple cible Rust (requis par tauri externalBin).
 *
 * Usage : npm run prepare-bins
 * À lancer sur chaque machine de build avant npm run tauri build.
 */

import { execSync } from "child_process";
import { copyFileSync, chmodSync, mkdirSync, existsSync } from "fs";
import { createWriteStream } from "fs";
import { join, dirname } from "path";
import { fileURLToPath } from "url";
import { createRequire } from "module";
import https from "https";

const require = createRequire(import.meta.url);
const __dirname = dirname(fileURLToPath(import.meta.url));
const binDir = join(__dirname, "..", "src-tauri", "binaries");
mkdirSync(binDir, { recursive: true });

// Détecte le triple cible depuis rustc (ex: aarch64-apple-darwin)
const rustInfo = execSync("rustc -vV", { encoding: "utf8" });
const triple = /host: (\S+)/.exec(rustInfo)?.[1];
if (!triple) throw new Error("Impossible de détecter le triple Rust (rustc absent ?)");
console.log(`Triple cible : ${triple}`);

const isWin = process.platform === "win32";
const ext = isWin ? ".exe" : "";

// ── ffmpeg ─────────────────────────────────────────────────────────────────
const ffmpegSrc = require("ffmpeg-static");
const ffmpegDest = join(binDir, `ffmpeg-${triple}${ext}`);
copyFileSync(ffmpegSrc, ffmpegDest);
if (!isWin) chmodSync(ffmpegDest, 0o755);
console.log(`✓ ffmpeg  → ${ffmpegDest}`);

// ── ffprobe ────────────────────────────────────────────────────────────────
const { path: ffprobeSrc } = require("@ffprobe-installer/ffprobe");
const ffprobeDest = join(binDir, `ffprobe-${triple}${ext}`);
copyFileSync(ffprobeSrc, ffprobeDest);
if (!isWin) chmodSync(ffprobeDest, 0o755);
console.log(`✓ ffprobe → ${ffprobeDest}`);

// ── yt-dlp ─────────────────────────────────────────────────────────────────
function ytdlpUrl() {
  if (isWin) return "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe";
  if (process.platform === "darwin") return "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp_macos";
  return "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp_linux";
}

function download(url, dest) {
  return new Promise((resolve, reject) => {
    const follow = (u) => {
      https
        .get(u, (res) => {
          if (res.statusCode >= 300 && res.statusCode < 400 && res.headers.location) {
            follow(res.headers.location);
            return;
          }
          if (res.statusCode !== 200) {
            reject(new Error(`HTTP ${res.statusCode} pour ${u}`));
            return;
          }
          const file = createWriteStream(dest);
          res.pipe(file);
          file.on("finish", () => { file.close(); resolve(); });
          file.on("error", reject);
        })
        .on("error", reject);
    };
    follow(url);
  });
}

const ytdlpDest = join(binDir, `yt-dlp-${triple}${ext}`);
if (existsSync(ytdlpDest)) {
  console.log(`⚡ yt-dlp déjà présent, téléchargement ignoré.`);
} else {
  const url = ytdlpUrl();
  process.stdout.write(`⬇  yt-dlp (${url}) … `);
  await download(url, ytdlpDest);
  if (!isWin) chmodSync(ytdlpDest, 0o755);
  console.log(`✓ → ${ytdlpDest}`);
}

console.log("\nTous les binaires sont prêts. Lance npm run tauri build.");
