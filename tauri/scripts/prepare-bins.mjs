/**
 * Télécharge / copie yt-dlp, ffmpeg, ffprobe dans src-tauri/binaries/
 * nommés avec le triple cible Rust (requis par tauri externalBin).
 *
 * Usage : npm run prepare-bins
 * À lancer sur chaque machine de build avant npm run tauri build.
 */

import { execSync } from "child_process";
import { copyFileSync, chmodSync, mkdirSync, existsSync, renameSync, unlinkSync } from "fs";
import { createWriteStream } from "fs";
import { join, dirname } from "path";
import { fileURLToPath } from "url";
import { createRequire } from "module";
import https from "https";

const require = createRequire(import.meta.url);
const __dirname = dirname(fileURLToPath(import.meta.url));
const binDir = join(__dirname, "..", "src-tauri", "binaries");
mkdirSync(binDir, { recursive: true });

// ── Mode universal2 (macOS Intel + Apple Silicon en un seul .app/.dmg) ───────
// Active par ROBLOADER_UNIVERSAL=1 (CI macOS, runner Apple Silicon uniquement).
// Produit des binaires « fat » nommes pour le triple `universal-apple-darwin`,
// requis par `tauri build --target universal-apple-darwin`. Evite le runner
// macos-13 Intel (en retrait chez GitHub), comme la chaine V1.
if (process.env.ROBLOADER_UNIVERSAL === "1") {
  if (process.platform !== "darwin") {
    throw new Error("ROBLOADER_UNIVERSAL=1 n'a de sens que sur macOS.");
  }
  const utriple = "universal-apple-darwin";
  console.log(`Mode universal2 → ${utriple}`);
  const work = join(binDir, "_universal_tmp");
  execSync(`rm -rf "${work}"`);
  mkdirSync(work, { recursive: true });

  const sh = (cmd) => execSync(cmd, { stdio: ["ignore", "pipe", "pipe"] }).toString().trim();
  const lipo = (a, b, out) => {
    execSync(`lipo -create "${a}" "${b}" -output "${out}"`);
    chmodSync(out, 0o755);
  };
  // npm pack d'un paquet par-arch (npm pack ne filtre pas os/cpu), extraction,
  // puis recherche du binaire `name` dans l'arborescence extraite.
  const fetchBin = (spec, name) => {
    const tgz = join(work, sh(`npm pack ${spec} --silent --pack-destination "${work}"`).split("\n").pop());
    const dest = join(work, spec.replace(/[^a-z0-9]+/gi, "_"));
    mkdirSync(dest, { recursive: true });
    execSync(`tar -xzf "${tgz}" -C "${dest}"`);
    const found = sh(`find "${dest}" -type f -name "${name}"`).split("\n").filter(Boolean)[0];
    if (!found) throw new Error(`Binaire ${name} introuvable dans ${spec}`);
    return found;
  };

  // ffmpeg / ffprobe : fusion des paquets installer par-arch.
  lipo(fetchBin("@ffmpeg-installer/darwin-arm64", "ffmpeg"),
       fetchBin("@ffmpeg-installer/darwin-x64", "ffmpeg"),
       join(binDir, `ffmpeg-${utriple}`));
  console.log("✓ ffmpeg  (universal2)");
  lipo(fetchBin("@ffprobe-installer/darwin-arm64", "ffprobe"),
       fetchBin("@ffprobe-installer/darwin-x64", "ffprobe"),
       join(binDir, `ffprobe-${utriple}`));
  console.log("✓ ffprobe (universal2)");

  // yt-dlp : yt-dlp_macos est deja un binaire universal2.
  const ytdlpUniv = join(binDir, `yt-dlp-${utriple}`);
  await download("https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp_macos", ytdlpUniv);
  chmodSync(ytdlpUniv, 0o755);
  console.log("✓ yt-dlp  (universal2)");

  // deno : fusion des deux zips par-arch.
  const denoArch = async (arch) => {
    const zip = join(work, `deno-${arch}.zip`);
    await download(`https://github.com/denoland/deno/releases/latest/download/deno-${arch}-apple-darwin.zip`, zip);
    const d = join(work, `deno_${arch}`);
    mkdirSync(d, { recursive: true });
    execSync(`unzip -o "${zip}" -d "${d}"`, { stdio: "pipe" });
    return join(d, "deno");
  };
  lipo(await denoArch("aarch64"), await denoArch("x86_64"), join(binDir, `deno-${utriple}`));
  console.log("✓ deno    (universal2)");

  execSync(`rm -rf "${work}"`);
  console.log("\nBinaires universal2 prêts.");
  process.exit(0);
}

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

// ── deno ───────────────────────────────────────────────────────────────────
// Deno distribue ses binaires en ZIP. On détecte l'archi du triple.
const denoDest = join(binDir, `deno-${triple}${ext}`);
if (existsSync(denoDest)) {
  console.log(`⚡ deno déjà présent, téléchargement ignoré.`);
} else {
  // Mapping triple Rust → nom de release Deno (même convention de triple).
  const denoZipUrl = `https://github.com/denoland/deno/releases/latest/download/deno-${triple}.zip`;
  const zipPath = denoDest + ".zip";
  process.stdout.write(`⬇  deno (${denoZipUrl}) … `);
  await download(denoZipUrl, zipPath);

  // Dézip : unzip (macOS/Linux) ou PowerShell (Windows).
  const tmpDir = join(binDir, `deno_unzip_${triple}`);
  mkdirSync(tmpDir, { recursive: true });
  if (isWin) {
    execSync(`powershell -Command "Expand-Archive -Force '${zipPath}' '${tmpDir}'"`, { stdio: "pipe" });
  } else {
    execSync(`unzip -o "${zipPath}" -d "${tmpDir}"`, { stdio: "pipe" });
  }
  const extracted = join(tmpDir, isWin ? "deno.exe" : "deno");
  renameSync(extracted, denoDest);
  if (!isWin) chmodSync(denoDest, 0o755);
  try { unlinkSync(zipPath); } catch {}
  try { execSync(`rm -rf "${tmpDir}"`); } catch {}
  console.log(`✓ → ${denoDest}`);
}

console.log("\nTous les binaires sont prêts.");
process.exit(0);
