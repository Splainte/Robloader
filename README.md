# ⚙️ Robloader

*[Version française](README.fr.md)*

**Download videos from YouTube, TikTok, Instagram, Twitter (X) and Weibo — in one click.**

Paste a URL: Robloader detects the site, downloads the video and hands you a ready-to-use file.

---

## 📥 Installation

No setup, no dependencies to install. Download the latest version for your system:

### → [Download the latest version](https://github.com/Splainte/Robloader/releases/latest)

- **Windows**: run `Robloader-Setup-Windows.exe` and follow the installer, which lets you pick English or French.

  *(Windows will warn you because the app is not signed: click "More info", then "Run anyway".)*
- **macOS**: open `Robloader-Setup-macOS.dmg` and drag the app into **Applications**.
  Works on Apple Silicon and Intel!

  *(On first launch, macOS will block the app because it is not signed: allow it from the Privacy & Security section of System Settings.)*

Everything you need is bundled with the app.

---

## ✨ Features

- **Multi-platform** — YouTube, TikTok, Instagram, X (Twitter) and Weibo. Paste the URL, click, done.
- **English or French** — chosen at install time (or on first launch on macOS), and changeable anytime in Settings.
- **Follows your accent color** — the app picks up your system accent color for its buttons.
- **Clip extraction** — grab only part of the video with a start and end timecode, or the whole video.
- **Quality choice** (YouTube) — Max (up to 4K), 1440p, 1080p, 720p, 480p.
- **Ready for Premiere Pro** — HEVC (H.265) output for the best quality/size trade-off, or ProRes for smooth editing.
- **GPU-accelerated encoding** — uses the graphics card (NVIDIA / Apple Silicon) when possible, with automatic fallback to the CPU.
- **Download queue** — chain several downloads, each one can be cancelled at any time.
- **Automatic cookies** — signs in to your account through your browser to unlock 4K, restricted videos or private content (Instagram, X); a **"Repair"** button explains what to do if needed.
- **Automatic updates** — tells you when a new version is out and installs it in a few clicks.

---

## ▶️ Usage

1. Paste the video **URL** (YouTube, TikTok, Instagram, X, Weibo).
2. *(Optional)* set a **Start** and **End** to keep only a clip.
3. Choose the **destination** (default: your Downloads folder).
4. Click **Download** — your file is ready. 🎬

The language is in **Settings**: the gear at the bottom of the side panel, or **Robloader › Settings… (⌘,)** on macOS.

---

## ❓ Having trouble?

Most issues (video asking you to sign in, private content, 4K not working) are solved by being **signed in to the site in your browser**: Robloader then uses your session automatically. The **"Repair"** button in the app explains the steps.

---

## 📄 License

Robloader is released under the [MIT License](LICENSE).

It bundles third-party tools that keep their own licenses: [yt-dlp](https://github.com/yt-dlp/yt-dlp) (Unlicense), [FFmpeg](https://ffmpeg.org) (LGPL/GPL) and [Deno](https://deno.com) (MIT).
