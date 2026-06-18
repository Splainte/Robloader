import { useMemo, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import "./App.css";
import { Icon } from "./icons";

const appWindow = getCurrentWindow();
// macOS = barre native (feux tricolores dessines par l'OS, en haut a gauche).
// Windows = frameless + boutons custom a droite.
const isMac = navigator.userAgent.includes("Macintosh");

// ------------------------------------------------------------------
// Donnees de l'UI (reprises de Robloader.py — purement visuel ici)
// ------------------------------------------------------------------
const QUALITY_LABELS = [
  "Qualité max (jusqu'à 4K)",
  "1440p (2K)",
  "1080p (Full HD)",
  "720p (HD)",
  "480p",
];

const OUTPUTS_TRANSCODE = [
  "HEVC",
  "ProRes",
  "Audio MP3",
  "Audio WAV",
  "Sous-titres seuls (.srt)",
];
const OUTPUTS_NATIVE = [
  "Vidéo (natif)",
  "Audio MP3",
  "Audio WAV",
  "Sous-titres seuls (.srt)",
];

// Profils de source : pilotent placeholder + options visibles (cf. SITE_PROFILES).
type Profile = {
  id: string;
  label: string;
  domains: string[];
  placeholder: string;
  ladder: boolean;
  subtitles: boolean;
  thumbnail: boolean;
};
const DEFAULT_PROFILE: Profile = {
  id: "default",
  label: "Vidéo",
  domains: [],
  placeholder: "Colle un lien (YouTube, TikTok, Instagram, X, Weibo)…",
  ladder: true,
  subtitles: true,
  thumbnail: true,
};
const SITE_PROFILES: Profile[] = [
  { id: "youtube", label: "YouTube", domains: ["youtube.com", "youtu.be"], placeholder: "Colle un lien YouTube ici…", ladder: true, subtitles: true, thumbnail: true },
  { id: "tiktok", label: "TikTok", domains: ["tiktok.com"], placeholder: "Colle un lien TikTok ici…", ladder: false, subtitles: false, thumbnail: true },
  { id: "instagram", label: "Instagram", domains: ["instagram.com", "instagr.am"], placeholder: "Colle un lien Instagram ici…", ladder: false, subtitles: false, thumbnail: false },
  { id: "x", label: "X", domains: ["twitter.com", "x.com"], placeholder: "Colle un lien X (Twitter) ici…", ladder: false, subtitles: false, thumbnail: false },
  { id: "weibo", label: "Weibo", domains: ["weibo.com", "weibo.cn"], placeholder: "Colle un lien Weibo ici…", ladder: false, subtitles: false, thumbnail: false },
];

function detectProfile(url: string): Profile {
  const u = url.trim();
  if (!u) return DEFAULT_PROFILE;
  let host = "";
  try {
    host = new URL(u.includes("://") ? u : "http://" + u).hostname.toLowerCase();
  } catch {
    return DEFAULT_PROFILE;
  }
  for (const p of SITE_PROFILES) {
    for (const d of p.domains) {
      if (host === d || host.endsWith("." + d)) return p;
    }
  }
  return DEFAULT_PROFILE;
}

// ------------------------------------------------------------------
// Petits composants reutilisables, stylises via .app[data-os]
// ------------------------------------------------------------------
function Select({
  value,
  values,
  onChange,
  ariaLabel,
}: {
  value: string;
  values: string[];
  onChange: (v: string) => void;
  ariaLabel: string;
}) {
  return (
    <div className="select">
      <select
        className="select__native"
        value={value}
        aria-label={ariaLabel}
        onChange={(e) => onChange(e.target.value)}
      >
        {values.map((v) => (
          <option key={v} value={v}>
            {v}
          </option>
        ))}
      </select>
      <Icon name="chevron" className="select__chevron" />
    </div>
  );
}

function Check({
  checked,
  onChange,
  label,
}: {
  checked: boolean;
  onChange: (v: boolean) => void;
  label: string;
}) {
  return (
    <label className="check">
      <input
        type="checkbox"
        checked={checked}
        onChange={(e) => onChange(e.target.checked)}
      />
      <span className="check__box">
        <Icon name="check" className="check__mark" />
      </span>
      <span className="check__label">{label}</span>
    </label>
  );
}

function App() {
  const [url, setUrl] = useState("");
  const [quality, setQuality] = useState(QUALITY_LABELS[0]);
  const [transcode, setTranscode] = useState(true);
  const [output, setOutput] = useState(OUTPUTS_TRANSCODE[0]);
  const [subs, setSubs] = useState(false);
  const [thumb, setThumb] = useState(false);
  const [start, setStart] = useState("");
  const [end, setEnd] = useState("");

  const profile = useMemo(() => detectProfile(url), [url]);
  const outputs = transcode ? OUTPUTS_TRANSCODE : OUTPUTS_NATIVE;

  // Si on bascule le transcodage et que le format courant n'existe plus, on retombe sur le 1er.
  function toggleTranscode(v: boolean) {
    setTranscode(v);
    const list = v ? OUTPUTS_TRANSCODE : OUTPUTS_NATIVE;
    if (!list.includes(output)) setOutput(list[0]);
  }

  return (
    <div className="app" data-os={isMac ? "macos" : "windows"}>
      {/* Zone de titre deplacable. macOS : place a gauche pour les feux natifs.
          Windows : on dessine nos propres boutons a droite. */}
      <header className="titlebar" data-tauri-drag-region>
        <span className="titlebar__title" data-tauri-drag-region>
          Robloader
        </span>

        {!isMac && (
          <div className="titlebar__controls">
            <button className="winbtn" aria-label="Reduire" onClick={() => appWindow.minimize()}>
              <svg width="11" height="11" viewBox="0 0 11 11">
                <rect x="1.5" y="5" width="8" height="1" fill="currentColor" />
              </svg>
            </button>
            <button className="winbtn" aria-label="Agrandir" onClick={() => appWindow.toggleMaximize()}>
              <svg width="11" height="11" viewBox="0 0 11 11">
                <rect x="1.5" y="1.5" width="8" height="8" fill="none" stroke="currentColor" strokeWidth="1" />
              </svg>
            </button>
            <button className="winbtn winbtn--close" aria-label="Fermer" onClick={() => appWindow.close()}>
              <svg width="11" height="11" viewBox="0 0 11 11">
                <path d="M1.5 1.5 L9.5 9.5 M9.5 1.5 L1.5 9.5" stroke="currentColor" strokeWidth="1.1" />
              </svg>
            </button>
          </div>
        )}
      </header>

      <main className="content">
        {/* ---- Ligne 1 : URL + qualite + destination + telecharger ---- */}
        <div className="row row--main">
          <div className="field field--url">
            <Icon name="link" className="field__icon" />
            <input
              className="input"
              type="text"
              value={url}
              placeholder={profile.placeholder}
              onChange={(e) => setUrl(e.target.value)}
              spellCheck={false}
            />
            {profile.id !== "default" && (
              <span className="source-chip">{profile.label}</span>
            )}
          </div>

          {profile.ladder && (
            <Select value={quality} values={QUALITY_LABELS} onChange={setQuality} ariaLabel="Qualité" />
          )}

          <button className="btn btn--secondary">
            <Icon name="folder" className="btn__icon" />
            <span>Destination</span>
          </button>

          <button className="btn btn--accent">
            <Icon name="download" className="btn__icon" />
            <span>Télécharger</span>
          </button>
        </div>

        {/* ---- Ligne 2 : extrait optionnel ---- */}
        <div className="row row--clip">
          <span className="row__label">
            <Icon name="scissors" className="row__label-icon" />
            Extrait (optionnel)
          </span>
          <span className="mini-label">Début</span>
          <input
            className="input input--time"
            value={start}
            placeholder="00:00"
            onChange={(e) => setStart(e.target.value)}
            spellCheck={false}
          />
          <span className="mini-label">Fin</span>
          <input
            className="input input--time"
            value={end}
            placeholder="01:30"
            onChange={(e) => setEnd(e.target.value)}
            spellCheck={false}
          />
          <span className="hint">format MM:SS ou HH:MM:SS — laisser vide pour la vidéo entière</span>
        </div>

        {/* ---- Ligne 3 : transcodage + sortie + sous-titres + miniature ---- */}
        <div className="row row--options">
          <Check checked={transcode} onChange={toggleTranscode} label="Transcodage" />
          <span className="row__label">Sortie</span>
          <Select value={output} values={outputs} onChange={setOutput} ariaLabel="Format de sortie" />
          {profile.subtitles && (
            <Check checked={subs} onChange={setSubs} label="Sous-titres (.srt)" />
          )}
          {profile.thumbnail && (
            <Check checked={thumb} onChange={setThumb} label="Miniature" />
          )}
        </div>

        {/* ---- Ligne d'etat ---- */}
        <div className="status-line">
          <Icon name="folder-open" className="status-line__icon" />
          <span>~/Téléchargements</span>
          <span className="status-line__dot">·</span>
          <span>cookies ✓</span>
        </div>

        {/* ---- File de telechargements ---- */}
        <div className="queue-head">
          <h2 className="queue-head__title">File de téléchargements</h2>
          <button className="btn btn--secondary btn--sm">
            <Icon name="broom" className="btn__icon" />
            <span>Nettoyer la liste</span>
          </button>
        </div>

        <div className="queue">
          <div className="queue__empty">
            <Icon name="tray" className="queue__empty-icon" />
            <p>Aucun téléchargement pour l’instant.</p>
            <span>Colle un lien ci-dessus pour commencer.</span>
          </div>
        </div>
      </main>
    </div>
  );
}

export default App;
