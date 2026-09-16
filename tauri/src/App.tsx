import { useEffect, useMemo, useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { getVersion } from "@tauri-apps/api/app";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import "./App.css";
import { Icon } from "./icons";
import MacLayout from "./MacLayout";

const appWindow = getCurrentWindow();
// macOS = barre native (feux tricolores dessines par l'OS, en haut a gauche).
// Windows = frameless + boutons custom a droite.
const isMac = navigator.userAgent.includes("Macintosh");

// ------------------------------------------------------------------
// Donnees de l'UI (reprises de Robloader.py)
// ------------------------------------------------------------------
const QUALITY_LABELS = [
  "Qualité max (jusqu'à 4K)",
  "1440p (QHD)",
  "1080p (Full HD)",
  "720p (HD)",
  "480p",
];

const OUTPUTS_TRANSCODE = [
  "HEVC",
  "ProRes",
  "Audio WAV",
  "Sous-titres seuls (.srt)",
];
const OUTPUTS_NATIVE = [
  "Vidéo (natif)",
  "Audio WAV",
  "Sous-titres seuls (.srt)",
];

// Profils de source : pilotent placeholder + options visibles (cf. SITE_PROFILES).
export type Profile = {
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
// Etats des taches (file de telechargements) + evenements backend
// ------------------------------------------------------------------
export type StatusKind = "info" | "warn" | "ok" | "err";
export type ActionKind = "cancel" | "open" | "repair" | "none";

export type Task = {
  id: number;
  title: string;
  thumbnail?: string;
  status: string;
  statusKind: StatusKind;
  percent: number; // 0..1
  indeterminate: boolean;
  action: ActionKind;
  finalPath?: string;
};

// Mise a jour partielle envoyee par le backend (event "task://update").
type TaskUpdate = {
  id: number;
  title?: string;
  thumbnail?: string;
  status?: string;
  statusKind?: StatusKind;
  percent?: number;
  indeterminate?: boolean;
  action?: ActionKind;
  finalPath?: string;
  done?: boolean;
};

type DownloadSettings = {
  qualityLabel: string;
  start: string;
  end: string;
  output: string;
  subs: boolean;
  thumb: boolean;
  transcode: boolean;
};

export type EnvInfo = {
  downloadDir: string;
  cookiesOk: boolean;
  cookiesSource: string;
  jsRuntime: boolean;
};

// ------------------------------------------------------------------
// Select custom : menu stylise (Windows 11 / macOS), thematise clair/sombre,
// aligne PILE sous le bouton qui l'ouvre (corrige l'alignement macOS et le
// menu natif qui restait clair sous Windows).
// ------------------------------------------------------------------
function Select({
  value,
  values,
  onChange,
  ariaLabel,
  disabled,
}: {
  value: string;
  values: string[];
  onChange: (v: string) => void;
  ariaLabel: string;
  disabled?: boolean;
}) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const onDoc = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    document.addEventListener("mousedown", onDoc);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDoc);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);

  return (
    <div className={`select${open ? " is-open" : ""}`} ref={ref}>
      <button
        type="button"
        className="select__trigger"
        aria-label={ariaLabel}
        aria-haspopup="listbox"
        aria-expanded={open}
        disabled={disabled}
        onClick={() => setOpen((o) => !o)}
      >
        <span className="select__value">{value}</span>
        <Icon name="chevron" className="select__chevron" />
      </button>
      {open && (
        <div className="select__menu" role="listbox">
          {values.map((v) => (
            <button
              type="button"
              key={v}
              role="option"
              aria-selected={v === value}
              className={`select__option${v === value ? " is-selected" : ""}`}
              onClick={() => {
                onChange(v);
                setOpen(false);
              }}
            >
              <Icon name="check" className="select__option-check" />
              <span className="select__option-label">{v}</span>
            </button>
          ))}
        </div>
      )}
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

// Carte d'une tache (titre + statut + barre + bouton d'action).
function TaskCard({
  task,
  onCancel,
  onOpen,
  onRepair,
}: {
  task: Task;
  onCancel: (id: number) => void;
  onOpen: (path: string) => void;
  onRepair: () => void;
}) {
  return (
    <div className="task">
      <div className="task__main">
        <div className="task__title" title={task.title}>
          {task.title}
        </div>
        <div className={`task__status task__status--${task.statusKind}`}>
          {task.status}
        </div>
        <div
          className={`task__bar${task.indeterminate ? " is-indeterminate" : ""}${
            task.statusKind === "ok" ? " is-ok" : ""
          }`}
        >
          <div
            className="task__bar-fill"
            style={
              task.indeterminate
                ? undefined
                : { width: `${Math.round(task.percent * 100)}%` }
            }
          />
        </div>
      </div>

      <div className="task__action">
        {task.action === "cancel" && (
          <button className="btn btn--secondary btn--sm" onClick={() => onCancel(task.id)}>
            <Icon name="x" className="btn__icon" />
            <span>Annuler</span>
          </button>
        )}
        {task.action === "open" && (
          <button
            className="btn btn--sm btn--ok"
            onClick={() => task.finalPath && onOpen(task.finalPath)}
          >
            <Icon name="folder-open" className="btn__icon" />
            <span>Ouvrir le dossier</span>
          </button>
        )}
        {task.action === "repair" && (
          <button className="btn btn--accent btn--sm" onClick={onRepair}>
            <Icon name="wrench" className="btn__icon" />
            <span>Réparer</span>
          </button>
        )}
      </div>
    </div>
  );
}

// ------------------------------------------------------------------
// Test visuel (menu macOS) : file factice pour eprouver defilement,
// barre d'outils et flou. Ids negatifs = jamais envoyes au moteur.
// ------------------------------------------------------------------
const FAKE_TITLES = [
  "Test du Pixel 10 Pro : le meilleur photophone de l'année ?",
  "Galaxy Z Fold8 : prise en main",
  "iPhone 17 Pro vs Galaxy S26 Ultra : le comparatif",
  "Keynote Apple septembre 2026 : résumé en 12 minutes",
  "Test OnePlus 15 : la charge 120 W en conditions réelles",
  "Les meilleurs écouteurs à moins de 100 €",
  "Xiaomi 17 Ultra : le test complet",
  "Android 17 : les 10 nouveautés à connaître",
  "MacBook Air M5 : faut-il craquer ?",
  "Nothing Phone (4) démonté pièce par pièce",
];

function fakeThumb(hue: number) {
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="160" height="90"><defs><linearGradient id="g" x2="1" y2="1"><stop offset="0" stop-color="hsl(${hue},70%,58%)"/><stop offset="1" stop-color="hsl(${(hue + 40) % 360},60%,30%)"/></linearGradient></defs><rect width="160" height="90" fill="url(#g)"/></svg>`;
  return `data:image/svg+xml,${encodeURIComponent(svg)}`;
}

function makeVisualTestTasks(): Task[] {
  const out: Task[] = [];
  for (let i = 0; i < 40; i++) {
    const kind = i % 8;
    const base: Task = {
      id: -(Date.now() % 1_000_000) * 100 - i - 1,
      title: `${FAKE_TITLES[i % FAKE_TITLES.length]}${i >= FAKE_TITLES.length ? ` (${i + 1})` : ""}`,
      thumbnail: i % 5 === 4 ? undefined : fakeThumb((i * 37) % 360),
      status: "",
      statusKind: "info",
      percent: 0,
      indeterminate: false,
      action: "cancel",
    };
    if (kind === 0) out.push({ ...base, status: "Téléchargement… 0%", percent: (i * 7) % 60 / 100 });
    else if (kind === 1) out.push({ ...base, status: "Conversion H.265… 0%", percent: (i * 5) % 50 / 100 });
    else if (kind === 2) out.push({ ...base, status: "Analyse de la vidéo…", indeterminate: true });
    else if (kind === 3) out.push({ ...base, status: "Échec : vidéo réservée aux membres", statusKind: "err", action: "repair" });
    else if (kind === 4) out.push({ ...base, status: "Annulé", statusKind: "warn", action: "none" });
    else out.push({ ...base, status: "Terminé ✓ Vidéo native (.mp4)", statusKind: "ok", percent: 1, action: "open", finalPath: "/tmp" });
  }
  return out;
}

function tickVisualTest(t: Task): Task {
  if (t.id >= 0 || t.action !== "cancel" || t.indeterminate) return t;
  const dl = t.status.startsWith("Téléchargement");
  const p = Math.min(1, t.percent + (dl ? 0.012 : 0.007));
  if (p >= 1) {
    return dl
      ? { ...t, percent: 0, status: "Conversion H.265… 0%" }
      : { ...t, percent: 1, status: "Terminé ✓ HEVC", statusKind: "ok", action: "open", finalPath: "/tmp" };
  }
  const label = dl ? "Téléchargement…" : "Conversion H.265…";
  return { ...t, percent: p, status: `${label} ${Math.round(p * 100)}%` };
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

  const [tasks, setTasks] = useState<Task[]>([]);
  const [env, setEnv] = useState<EnvInfo | null>(null);
  type UpdateInfo = { version: string; url: string };
  const [availableUpdate, setAvailableUpdate] = useState<UpdateInfo | null>(null);
  const [updateInstalling, setUpdateInstalling] = useState(false);
  const [appVersion, setAppVersion] = useState("");
  const idCounter = useRef(0);

  const profile = useMemo(() => detectProfile(url), [url]);
  const outputs = transcode ? OUTPUTS_TRANSCODE : OUTPUTS_NATIVE;

  // La fenetre est creee cachee (tauri.conf.json) : on l'affiche apres le
  // premier rendu peint, pour que fond, barre et volet apparaissent ensemble.
  useEffect(() => {
    requestAnimationFrame(() =>
      requestAnimationFrame(() => {
        appWindow.show().catch(() => {});
      })
    );
  }, []);

  // Infos d'environnement (dossier, cookies, runtime JS) pour la ligne d'etat.
  useEffect(() => {
    invoke<EnvInfo>("get_env").then(setEnv).catch(() => {});
  }, []);

  // Numero de version affiche a cote du titre.
  useEffect(() => {
    getVersion().then(setAppVersion).catch(() => {});
  }, []);

  // Le mot-cle CSS `AccentColor` ne suit l'accent systeme ni dans WKWebView
  // (macOS) ni dans WebView2 (Windows, ou il rend un bleu fixe) : on lit la
  // couleur exacte depuis le backend, et on la relit a chaque regain de focus
  // (= retour depuis les Reglages) pour suivre un changement a chaud.
  useEffect(() => {
    const applyAccent = () =>
      invoke<string | null>("get_accent_color").then((c) => {
        if (!c) return;
        const root = document.documentElement;
        root.style.setProperty("--rl-accent", c);
        // Texte pose SUR l'accent : noir si l'accent est clair (jaune...),
        // sinon du blanc y serait illisible.
        const [r, g, b] = [1, 3, 5].map((i) => parseInt(c.slice(i, i + 2), 16));
        const lum = (0.299 * r + 0.587 * g + 0.114 * b) / 255;
        root.style.setProperty("--rl-accent-text", lum > 0.6 ? "#000000" : "#ffffff");
      }).catch(() => {});
    applyAccent();
    const unlisten = appWindow.onFocusChanged(({ payload: focused }) => {
      if (focused) applyAccent();
    });
    return () => { unlisten.then((f) => f()); };
  }, []);

  // Verif mise a jour unique au lancement — pas de polling.
  useEffect(() => {
    invoke<UpdateInfo | null>("check_update").then((u) => { if (u) setAvailableUpdate(u); }).catch(() => {});
  }, []);

  // Ecoute des mises a jour de taches emises par le backend.
  useEffect(() => {
    const un = listen<TaskUpdate>("task://update", (e) => {
      const u = e.payload;
      setTasks((prev) =>
        prev.map((t) =>
          t.id === u.id
            ? {
                ...t,
                ...(u.title !== undefined ? { title: u.title } : {}),
                ...(u.thumbnail !== undefined ? { thumbnail: u.thumbnail } : {}),
                ...(u.status !== undefined ? { status: u.status } : {}),
                ...(u.statusKind !== undefined ? { statusKind: u.statusKind } : {}),
                ...(u.percent !== undefined ? { percent: u.percent } : {}),
                ...(u.indeterminate !== undefined ? { indeterminate: u.indeterminate } : {}),
                ...(u.action !== undefined ? { action: u.action } : {}),
                ...(u.finalPath !== undefined ? { finalPath: u.finalPath } : {}),
              }
            : t
        )
      );
    });
    return () => {
      un.then((f) => f());
    };
  }, []);

  // ---- macOS : pont avec la barre d'outils et le volet natifs (macos_ui.rs) ----
  // Les reglages vivent cote natif ; on recoit lien + reglages au clic sur
  // Telecharger. Refs = toujours la derniere version des handlers.
  const macHandlers = useRef({
    download: (_u: string, _s: DownloadSettings) => {},
    chooseDestination: () => {},
    revealDestination: () => {},
    repair: () => {},
    installUpdate: () => {},
  });
  macHandlers.current = {
    download: startDownloadWith,
    chooseDestination,
    revealDestination: () => env && openFolder(env.downloadDir),
    repair,
    installUpdate,
  };
  useEffect(() => {
    if (!isMac) return;
    const subs = [
      listen<{ url: string; settings: DownloadSettings }>("mac://download", (e) =>
        macHandlers.current.download(e.payload.url, e.payload.settings)
      ),
      listen("mac://choose-destination", () => macHandlers.current.chooseDestination()),
      listen("mac://reveal-destination", () => macHandlers.current.revealDestination()),
      listen("mac://repair", () => macHandlers.current.repair()),
      listen("mac://install-update", () => macHandlers.current.installUpdate()),
      listen<boolean>("mac://visual-test", (e) =>
        setTasks((prev) => [
          ...(e.payload ? makeVisualTestTasks() : []),
          ...prev.filter((t) => t.id > 0),
        ])
      ),
    ];
    return () => subs.forEach((p) => p.then((f) => f()));
  }, []);
  useEffect(() => {
    if (isMac && env) invoke("mac_set_env", { ...env }).catch(() => {});
  }, [env]);
  useEffect(() => {
    if (!isMac) return;
    invoke("mac_set_update", {
      version: availableUpdate?.version ?? null,
      installing: updateInstalling,
    }).catch(() => {});
  }, [availableUpdate, updateInstalling]);

  // Si on bascule le transcodage et que le format courant n'existe plus, on retombe sur le 1er.
  function toggleTranscode(v: boolean) {
    setTranscode(v);
    const list = v ? OUTPUTS_TRANSCODE : OUTPUTS_NATIVE;
    if (!list.includes(output)) setOutput(list[0]);
  }

  function startDownload() {
    startDownloadWith(url, {
      qualityLabel: quality,
      start,
      end,
      output,
      subs: profile.subtitles ? subs : false,
      thumb: profile.thumbnail ? thumb : false,
      transcode,
    });
  }

  function startDownloadWith(rawUrl: string, s: DownloadSettings) {
    const raw = rawUrl.trim();
    if (!raw) return;

    // Batch : plusieurs URLs separees par des espaces / retours a la ligne.
    const urls = raw.split(/\s+/).filter((u) => u.startsWith("http"));
    const list = urls.length ? urls : [raw];

    const newTasks: Task[] = [];
    for (const u of list) {
      idCounter.current += 1;
      const id = idCounter.current;
      newTasks.push({
        id,
        title: "Analyse du lien…",
        status: "En attente…",
        statusKind: "info",
        percent: 0,
        indeterminate: true,
        action: "cancel",
      });
      invoke("start_download", {
        opts: {
          id,
          url: u,
          ...s,
          downloadDir: env?.downloadDir ?? null,
        },
      }).catch((err) => {
        setTasks((prev) =>
          prev.map((t) =>
            t.id === id
              ? { ...t, status: `Échec : ${err}`, statusKind: "err", action: "none", indeterminate: false }
              : t
          )
        );
      });
    }
    setTasks((prev) => [...newTasks.reverse(), ...prev]);

    setUrl("");
    setStart("");
    setEnd("");
  }

  // Test visuel : les taches factices (id < 0) progressent toutes seules.
  const hasFakeRunning = tasks.some((t) => t.id < 0 && t.action === "cancel");
  useEffect(() => {
    if (!hasFakeRunning) return;
    const timer = window.setInterval(() => setTasks((prev) => prev.map(tickVisualTest)), 400);
    return () => window.clearInterval(timer);
  }, [hasFakeRunning]);

  function cancelTask(id: number) {
    if (id < 0) {
      setTasks((prev) =>
        prev.map((t) =>
          t.id === id ? { ...t, status: "Annulé", statusKind: "warn", action: "none", indeterminate: false } : t
        )
      );
      return;
    }
    invoke("cancel_download", { id }).catch(() => {});
  }
  function openFolder(path: string) {
    invoke("reveal_in_folder", { path }).catch(() => {});
  }
  function repair() {
    invoke("open_cookie_help").catch(() => {});
  }
  function clearList() {
    setTasks((prev) => prev.filter((t) => t.action === "cancel"));
  }
  async function installUpdate() {
    if (!availableUpdate) return;
    setUpdateInstalling(true);
    try {
      await invoke("install_update", { url: availableUpdate.url });
    } catch {
      setUpdateInstalling(false);
    }
  }
  async function chooseDestination() {
    const dir = await invoke<string | null>("choose_destination").catch(() => null);
    if (dir && env) setEnv({ ...env, downloadDir: dir });
  }

  if (isMac) {
    return (
      <MacLayout
        tasks={tasks}
        onCancel={cancelTask}
        onOpen={openFolder}
        onRepair={repair}
        onClear={clearList}
      />
    );
  }

  return (
    <div className="app" data-os={isMac ? "macos" : "windows"}>
      {/* Zone de titre deplacable. macOS : place a gauche pour les feux natifs.
          Windows : on dessine nos propres boutons a droite. */}
      <header className="titlebar" data-tauri-drag-region>
        <span className="titlebar__title" data-tauri-drag-region>
          Robloader
          {appVersion && <span className="titlebar__version">{appVersion}</span>}
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
              <svg width="11" height="11" viewBox="0 0 11 11" style={{ shapeRendering: "geometricPrecision" }}>
                <path d="M1.5 1.5 L9.5 9.5 M9.5 1.5 L1.5 9.5" stroke="currentColor" strokeWidth="1.1" strokeLinecap="round" />
              </svg>
            </button>
          </div>
        )}
      </header>

      {availableUpdate && (
        <div className="update-banner">
          <span>Mise à jour disponible — v{availableUpdate.version}</span>
          <button
            className="btn btn--accent btn--sm"
            disabled={updateInstalling}
            onClick={installUpdate}
          >
            {updateInstalling ? "Téléchargement…" : "Installer et relancer"}
          </button>
        </div>
      )}

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
              onKeyDown={(e) => e.key === "Enter" && startDownload()}
              spellCheck={false}
            />
            {profile.id !== "default" && (
              <span className="source-chip">{profile.label}</span>
            )}
          </div>

          {profile.ladder && (
            <Select value={quality} values={QUALITY_LABELS} onChange={setQuality} ariaLabel="Qualité" />
          )}

          <button className="btn btn--secondary" onClick={chooseDestination}>
            <Icon name="folder" className="btn__icon" />
            <span>Destination</span>
          </button>

          <button className="btn btn--accent" onClick={startDownload}>
            <Icon name="download" className="btn__icon" />
            <span>Télécharger</span>
          </button>
        </div>

        {/* ---- Ligne 2 : extrait optionnel ---- */}
        <div className="row row--clip">
          <span className="row__label">Extrait (optionnel)</span>
          <span className="mini-label">Début</span>
          <input
            className="input input--time"
            value={start}
            placeholder="00:00"
            onChange={(e) => setStart(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && startDownload()}
            spellCheck={false}
          />
          <span className="mini-label">Fin</span>
          <input
            className="input input--time"
            value={end}
            placeholder="01:30"
            onChange={(e) => setEnd(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && startDownload()}
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
          <span>{env?.downloadDir ?? "…"}</span>
          {env && (
            <>
              <span className="status-line__dot">·</span>
              <span>
                {env.cookiesOk
                  ? `cookies ${env.cookiesSource} ✓`
                  : "cookies absents"}
              </span>
              {!env.jsRuntime && (
                <>
                  <span className="status-line__dot">·</span>
                  <span>4K limitée (Deno absent)</span>
                </>
              )}
            </>
          )}
        </div>

        {/* ---- File de telechargements ---- */}
        <div className="queue-head">
          <h2 className="queue-head__title">File de téléchargements</h2>
          <button className="btn btn--secondary btn--sm" onClick={clearList}>
            <span>Nettoyer la liste</span>
          </button>
        </div>

        <div className="queue">
          {tasks.length === 0 ? (
            <div className="queue__empty">
              <Icon name="tray" className="queue__empty-icon" />
              <p>Aucun téléchargement pour l’instant.</p>
              <span>Colle un lien ci-dessus pour commencer.</span>
            </div>
          ) : (
            <div className="queue__list">
              {tasks.map((t) => (
                <TaskCard
                  key={t.id}
                  task={t}
                  onCancel={cancelTask}
                  onOpen={openFolder}
                  onRepair={repair}
                />
              ))}
            </div>
          )}
        </div>
      </main>
    </div>
  );
}

export default App;
