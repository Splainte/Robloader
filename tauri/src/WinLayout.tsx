/* ============================================================
   Windows 11 : meme organisation que le Mac (volet de reglages a gauche,
   file a droite), en Fluent Design. Le Mica est natif (window-vibrancy) ;
   les controles sont dessines en HTML au gabarit Fluent 2 (WebView2 ne sait
   pas afficher de vrais controles WinUI).
   ============================================================ */

import { useEffect, useLayoutEffect, useRef, useState, type KeyboardEvent as ReactKeyboardEvent } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { outputDisplay, qualityDisplay, type EnvInfo, type Profile, type Task } from "./App";
import appIcon from "./assets/app-icon.png";
import { languagePickNeeded, useT } from "./i18n";
import { FirstRunDialog, SettingsDialog } from "./SettingsDialog";
import "./WinLayout.css";

const appWindow = getCurrentWindow();

type Props = {
  url: string;
  setUrl: (v: string) => void;
  profile: Profile;
  qualities: string[];
  quality: string;
  setQuality: (v: string) => void;
  clip: boolean;
  setClip: (v: boolean) => void;
  start: string;
  setStart: (v: string) => void;
  end: string;
  setEnd: (v: string) => void;
  transcode: boolean;
  toggleTranscode: (v: boolean) => void;
  output: string;
  setOutput: (v: string) => void;
  outputs: string[];
  subs: boolean;
  setSubs: (v: boolean) => void;
  thumb: boolean;
  setThumb: (v: boolean) => void;
  tasks: Task[];
  env: EnvInfo | null;
  appVersion: string;
  updateVersion: string | null;
  updateInstalling: boolean;
  onInstallUpdate: () => void;
  onDownload: () => void;
  onCancel: (id: number) => void;
  onOpen: (path: string) => void;
  onRepair: () => void;
  onClear: () => void;
  onChooseDestination: () => void;
  hasVisualTest: boolean;
  onVisualTest: (fill: boolean) => void;
};

// ---------- Icones (trait fin facon Segoe Fluent Icons) ----------
const I = {
  link: <path d="M8.5 11.5a3 3 0 0 0 4.2 0l2.6-2.6a3 3 0 0 0-4.2-4.2l-1 1M11.5 8.5a3 3 0 0 0-4.2 0l-2.6 2.6a3 3 0 0 0 4.2 4.2l1-1" />,
  paste: <><rect x="4.5" y="3.5" width="11" height="14" rx="1.5" /><path d="M7.5 3.5V2.5h5v1M7.5 8h5M7.5 11h5" /></>,
  download: <path d="M10 3v10M5.5 8.5 10 13l4.5-4.5M4 16.5h12" />,
  scissors: <><circle cx="5" cy="5.5" r="2.5" /><circle cx="5" cy="14.5" r="2.5" /><path d="M7 7.2 17 15M7 12.8 17 5" /></>,
  convert: <path d="M3 7h11l-3-3M17 13H6l3 3" />,
  subtitles: <><rect x="2.5" y="4.5" width="15" height="11" rx="2" /><path d="M5.5 11.5h4M11.5 11.5h3M5.5 8.5h2M9.5 8.5h5" /></>,
  image: <><rect x="2.5" y="4.5" width="15" height="11" rx="2" /><path d="m2.5 13 4-4 4 4 2-2 5 4" /></>,
  folder: <path d="M2.5 6a1.5 1.5 0 0 1 1.5-1.5h3.5l2 2H16A1.5 1.5 0 0 1 17.5 8v6.5A1.5 1.5 0 0 1 16 16H4a1.5 1.5 0 0 1-1.5-1.5z" />,
  chevron: <path d="M5.5 8 10 12.5 14.5 8" />,
  check: <path d="M4.5 10.5 8 14l7.5-8" />,
  update: <path d="M15.5 8A6 6 0 0 0 4.3 6.5M4.5 12a6 6 0 0 0 11.2 1.5M15.5 3.5V8H11M4.5 16.5V12H9" />,
  settings: <><path d="M8.6 2.5h2.8l.4 2.1a5.6 5.6 0 0 1 1.5.9l2-.7 1.4 2.4-1.6 1.4a5.6 5.6 0 0 1 0 1.8l1.6 1.4-1.4 2.4-2-.7a5.6 5.6 0 0 1-1.5.9l-.4 2.1H8.6l-.4-2.1a5.6 5.6 0 0 1-1.5-.9l-2 .7-1.4-2.4 1.6-1.4a5.6 5.6 0 0 1 0-1.8L3.3 7.2l1.4-2.4 2 .7a5.6 5.6 0 0 1 1.5-.9z" /><circle cx="10" cy="10" r="2.3" /></>,
  globe: <><circle cx="10" cy="10" r="7.5" /><path d="M2.5 10h15M10 2.5c2.2 2.1 3.2 4.6 3.2 7.5s-1 5.4-3.2 7.5c-2.2-2.1-3.2-4.6-3.2-7.5s1-5.4 3.2-7.5z" /></>,
};
export function Ico({ d, className }: { d: keyof typeof I; className?: string }) {
  return (
    <svg className={`wi ${className ?? ""}`} viewBox="0 0 20 20" fill="none" stroke="currentColor" strokeWidth="1.25" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      {I[d]}
    </svg>
  );
}

function Toggle({ id, checked, onChange, label }: { id: string; checked: boolean; onChange: (v: boolean) => void; label: string }) {
  const t = useT();
  return (
    <>
      <span className="w-state">{checked ? t("pane.on") : t("pane.off")}</span>
      <button
        type="button"
        id={id}
        role="switch"
        aria-checked={checked}
        aria-label={label}
        className="w-toggle"
        onClick={() => onChange(!checked)}
      />
    </>
  );
}

// ComboBox Fluent : menu en position fixe (jamais rogne par le volet qui defile),
// ouvert centre sur l'element choisi comme sous Windows 11.
// `display` : libelle affiche de chaque valeur (traduction).
function ComboBox({ id, value, values, onChange, label, display = (v) => v }: { id: string; value: string; values: string[]; onChange: (v: string) => void; label: string; display?: (v: string) => string }) {
  const [open, setOpen] = useState(false);
  const [pos, setPos] = useState<{ left: number; top: number; width: number } | null>(null);
  const btn = useRef<HTMLButtonElement>(null);
  const menu = useRef<HTMLDivElement>(null);

  useLayoutEffect(() => {
    if (!open || !btn.current) return;
    const r = btn.current.getBoundingClientRect();
    const idx = Math.max(0, values.indexOf(value));
    const itemH = 36;
    let top = r.top - 4 - idx * itemH;
    const menuH = values.length * itemH + 8;
    top = Math.max(8, Math.min(top, window.innerHeight - menuH - 8));
    setPos({ left: r.left - 4, top, width: r.width + 8 });
  }, [open, value, values]);

  useEffect(() => {
    if (!open) return;
    const close = (e: Event) => {
      if (menu.current?.contains(e.target as Node) || btn.current?.contains(e.target as Node)) return;
      setOpen(false);
    };
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && setOpen(false);
    document.addEventListener("mousedown", close);
    document.addEventListener("keydown", onKey);
    window.addEventListener("resize", close);
    document.addEventListener("scroll", close, true);
    return () => {
      document.removeEventListener("mousedown", close);
      document.removeEventListener("keydown", onKey);
      window.removeEventListener("resize", close);
      document.removeEventListener("scroll", close, true);
    };
  }, [open]);

  return (
    <>
      <button
        type="button"
        id={id}
        ref={btn}
        className="w-combo"
        aria-label={label}
        aria-haspopup="listbox"
        aria-expanded={open}
        onClick={() => setOpen((o) => !o)}
      >
        <span>{display(value)}</span>
        <Ico d="chevron" />
      </button>
      {open && pos && (
        <div ref={menu} className="w-menu" role="listbox" style={{ left: pos.left, top: pos.top, minWidth: pos.width }}>
          {values.map((v) => (
            <button
              type="button"
              key={v}
              role="option"
              aria-selected={v === value}
              className="w-menu__item"
              onClick={() => {
                onChange(v);
                setOpen(false);
                btn.current?.focus();
              }}
            >
              {display(v)}
            </button>
          ))}
        </div>
      )}
    </>
  );
}

// Anneau de progression : accent au telechargement, violet au transcodage.
const RING_C = 2 * Math.PI * 9;
function Ring({ task }: { task: Task }) {
  const tc = !task.indeterminate && task.phase === "process";
  const p = task.indeterminate ? 0.25 : task.percent;
  return (
    <svg className={`w-ring${task.indeterminate ? " is-spinning" : ""}${tc ? " is-transcode" : ""}`} viewBox="0 0 22 22" aria-hidden="true">
      <circle className="w-ring__track" cx="11" cy="11" r="9" />
      <circle className="w-ring__fill" cx="11" cy="11" r="9" strokeDasharray={RING_C} strokeDashoffset={RING_C * (1 - p)} />
    </svg>
  );
}

function TaskRow({ task, onCancel, onOpen, onRepair }: { task: Task; onCancel: (id: number) => void; onOpen: (path: string) => void; onRepair: () => void }) {
  const t = useT();
  const [thumbOk, setThumbOk] = useState(true);
  return (
    <div className="w-item">
      <div className="w-item__thumb">
        {task.thumbnail && thumbOk ? (
          <img src={task.thumbnail} alt="" onError={() => setThumbOk(false)} />
        ) : (
          <Ico d="download" />
        )}
      </div>
      <div className="w-item__text">
        <div className="w-item__title" title={task.title}>{task.title}</div>
        <div className={`w-item__status w-item__status--${task.statusKind}`} title={task.status}>{task.status}</div>
      </div>
      <div className="w-item__action">
        {task.action === "cancel" && (
          <>
            <Ring task={task} />
            <button type="button" className="w-btn" onClick={() => onCancel(task.id)}>{t("task.cancel")}</button>
          </>
        )}
        {task.action === "open" && (
          <button type="button" className="w-btn" onClick={() => task.finalPath && onOpen(task.finalPath)}>{t("task.show")}</button>
        )}
        {task.action === "repair" && (
          <button type="button" className="w-btn w-btn--accent" onClick={onRepair}>{t("task.repair")}</button>
        )}
      </div>
    </div>
  );
}

export default function WinLayout(props: Props) {
  const {
    url, setUrl, profile, qualities, quality, setQuality,
    clip, setClip, start, setStart, end, setEnd,
    transcode, toggleTranscode, output, setOutput, outputs,
    subs, setSubs, thumb, setThumb, tasks, env, appVersion,
  } = props;

  const t = useT();
  const urlRef = useRef<HTMLInputElement>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);
  // Installe sans choix de langue (cas rare : l'installeur la demande).
  const [firstRun, setFirstRun] = useState(languagePickNeeded);
  const [toast, setToast] = useState("");
  const toastTimer = useRef<number | undefined>(undefined);
  const [maximized, setMaximized] = useState(false);
  const [ctx, setCtx] = useState<{ x: number; y: number } | null>(null);
  const ctxRef = useRef<HTMLDivElement>(null);

  // Pas de menu de WebView2 (Retour, Actualiser, Imprimer...) hors des champs
  // de saisie ; dans la file, notre propre menu contextuel.
  useEffect(() => {
    const onContext = (e: MouseEvent) => {
      const t = e.target as HTMLElement;
      if (t.closest("input, textarea")) return;
      e.preventDefault();
      if (t.closest(".w-list")) {
        const menuW = 240;
        const menuH = 90;
        setCtx({
          x: Math.min(e.clientX, window.innerWidth - menuW - 8),
          y: Math.min(e.clientY, window.innerHeight - menuH - 8),
        });
      } else {
        setCtx(null);
      }
    };
    document.addEventListener("contextmenu", onContext);
    return () => document.removeEventListener("contextmenu", onContext);
  }, []);

  useEffect(() => {
    if (!ctx) return;
    const close = (e: Event) => {
      if (ctxRef.current?.contains(e.target as Node)) return;
      setCtx(null);
    };
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && setCtx(null);
    document.addEventListener("mousedown", close);
    document.addEventListener("keydown", onKey);
    window.addEventListener("blur", close);
    document.addEventListener("scroll", close, true);
    return () => {
      document.removeEventListener("mousedown", close);
      document.removeEventListener("keydown", onKey);
      window.removeEventListener("blur", close);
      document.removeEventListener("scroll", close, true);
    };
  }, [ctx]);

  useEffect(() => {
    const sync = () => appWindow.isMaximized().then(setMaximized).catch(() => {});
    sync();
    const un = appWindow.onResized(sync);
    return () => {
      un.then((f) => f());
    };
  }, []);

  function showToast(msg: string) {
    setToast(msg);
    window.clearTimeout(toastTimer.current);
    toastTimer.current = window.setTimeout(() => setToast(""), 2400);
  }

  function download() {
    if (!url.trim()) {
      showToast(t("toast.pasteFirst"));
      urlRef.current?.focus();
      return;
    }
    props.onDownload();
  }

  async function paste() {
    try {
      const text = (await navigator.clipboard.readText()).trim();
      if (text) setUrl(text);
      urlRef.current?.focus();
    } catch {
      urlRef.current?.focus();
      showToast(t("toast.useCtrlV"));
    }
  }

  // Ctrl+L : focus dans le champ de lien.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.ctrlKey && !e.shiftKey && !e.altKey && e.key.toLowerCase() === "l") {
        e.preventDefault();
        urlRef.current?.focus();
        urlRef.current?.select();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const running = tasks.filter((t) => t.action === "cancel").length;
  const onEnter = (e: ReactKeyboardEvent) => e.key === "Enter" && download();
  const shortQuality = (q: string) => (q === props.qualities[0] ? "Max" : q.replace(/ \(.*\)$/, ""));

  return (
    <div className="app win" data-os="windows">
      {/* ---- Barre de titre (sur Mica) ---- */}
      <header className="w-titlebar" data-tauri-drag-region>
        <div className="w-titlebar__app" data-tauri-drag-region>
          <img className="w-titlebar__icon" src={appIcon} alt="" data-tauri-drag-region />
          <span data-tauri-drag-region>Robloader</span>
          {appVersion && <span className="w-titlebar__ver" data-tauri-drag-region>{appVersion}</span>}
        </div>
        <div className="w-caption">
          <button type="button" aria-label={t("win.minimize")} onClick={() => appWindow.minimize()}>
            <svg width="10" height="10" viewBox="0 0 10 10"><path d="M0 5.5h10" stroke="currentColor" /></svg>
          </button>
          <button type="button" aria-label={maximized ? t("win.restore") : t("win.maximize")} onClick={() => appWindow.toggleMaximize()}>
            {maximized ? (
              <svg width="10" height="10" viewBox="0 0 10 10" fill="none" stroke="currentColor"><rect x=".5" y="2.5" width="7" height="7" rx="1" /><path d="M2.5 2.5V1.5a1 1 0 0 1 1-1h5a1 1 0 0 1 1 1v5a1 1 0 0 1-1 1h-1" /></svg>
            ) : (
              <svg width="10" height="10" viewBox="0 0 10 10"><rect x=".5" y=".5" width="9" height="9" rx="1.5" fill="none" stroke="currentColor" /></svg>
            )}
          </button>
          <button type="button" className="w-caption__close" aria-label={t("win.close")} onClick={() => appWindow.close()}>
            <svg width="10" height="10" viewBox="0 0 10 10"><path d="M.5.5l9 9m0-9-9 9" stroke="currentColor" /></svg>
          </button>
        </div>
      </header>

      {/* ---- Volet de reglages (Mica) ---- */}
      <aside className="w-pane" aria-label={t("pane.label")}>
        {/* Reglages defilants ; le pied (cookies + roue) reste ancre en bas
            et le contenu s'efface en passant dessous. */}
        <div className="w-pane__scroll">
        {profile.ladder && (
          <section>
            <h3 id="w-quality-title">{t("pane.quality")}</h3>
            <div className="w-qgrid" role="radiogroup" aria-labelledby="w-quality-title">
              {qualities.map((q, i) => (
                <button
                  type="button"
                  key={q}
                  id={`w-quality-${i}`}
                  role="radio"
                  aria-checked={q === quality}
                  title={qualityDisplay(q)}
                  className={`w-btn${q === quality ? " w-btn--accent" : ""}`}
                  onClick={() => setQuality(q)}
                >
                  {shortQuality(q)}
                </button>
              ))}
            </div>
          </section>
        )}

        <section>
          <h3>{t("pane.options")}</h3>
          <div className="w-cards">
            <div className="w-card">
              <Ico d="scissors" className="w-card__icon" />
              <span className="w-card__label">{t("pane.clip")}</span>
              <Toggle id="w-clip" checked={clip} onChange={setClip} label={t("pane.clip")} />
            </div>
            {clip && (
              <div className="w-card w-card--times">
                <label className="w-times" htmlFor="w-start">
                  {t("pane.start")}
                  <input id="w-start" className="w-field w-field--time" value={start} placeholder="00:00" onChange={(e) => setStart(e.target.value)} onKeyDown={onEnter} spellCheck={false} />
                </label>
                <label className="w-times" htmlFor="w-end">
                  {t("pane.end")}
                  <input id="w-end" className="w-field w-field--time" value={end} placeholder="01:30" onChange={(e) => setEnd(e.target.value)} onKeyDown={onEnter} spellCheck={false} />
                </label>
              </div>
            )}
            <div className="w-card">
              <Ico d="convert" className="w-card__icon" />
              <span className="w-card__label">{t("pane.transcode")}</span>
              <Toggle id="w-transcode" checked={transcode} onChange={toggleTranscode} label={t("pane.transcode")} />
            </div>
            {transcode && (
              <div className="w-card w-card--sub">
                <span className="w-card__label">{t("pane.output")}</span>
                <ComboBox id="w-output" value={output} values={outputs} onChange={setOutput} label={t("pane.output")} display={outputDisplay} />
              </div>
            )}
            {profile.subtitles && (
              <div className="w-card">
                <Ico d="subtitles" className="w-card__icon" />
                <span className="w-card__label">{t("pane.subs")}</span>
                <Toggle id="w-subs" checked={subs} onChange={setSubs} label={t("pane.subs")} />
              </div>
            )}
            {profile.thumbnail && (
              <div className="w-card">
                <Ico d="image" className="w-card__icon" />
                <span className="w-card__label">{t("pane.thumb")}</span>
                <Toggle id="w-thumb" checked={thumb} onChange={setThumb} label={t("pane.thumb")} />
              </div>
            )}
          </div>
        </section>

        <section>
          <h3>{t("pane.destination")}</h3>
          <div className="w-dest">
            <div className="w-dest__path" title={env?.downloadDir}>
              <Ico d="folder" />
              <span>{env?.downloadDir ?? "…"}</span>
            </div>
            <div className="w-dest__buttons">
              <button type="button" id="w-choose" className="w-btn" onClick={props.onChooseDestination}>{t("pane.choose")}</button>
              <button type="button" id="w-reveal" className="w-btn" disabled={!env} onClick={() => env && props.onOpen(env.downloadDir)}>{t("pane.open")}</button>
            </div>
          </div>
        </section>
        </div>

        <footer className="w-pane__foot">
          <div className="w-pane__state">
            {env && (env.cookiesOk ? (
              <span><i className="w-dot" />{t("pane.cookiesOk", { source: env.cookiesSource })}</span>
            ) : (
              <button type="button" className="w-link" onClick={props.onRepair}>{t("pane.cookiesMissing")}</button>
            ))}
            {env && !env.jsRuntime && <span>{t("pane.noDeno")}</span>}
          </div>
          <button
            type="button"
            id="w-settings"
            className={`w-btn w-btn--subtle w-btn--icon${settingsOpen ? " is-active" : ""}`}
            aria-label={t("settings.title")}
            title={t("settings.title")}
            onClick={() => setSettingsOpen(true)}
          >
            <Ico d="settings" />
          </button>
        </footer>
      </aside>

      {/* ---- Couche de contenu : lien + file ---- */}
      <main className="w-layer">
        <div className="w-cmd">
          <label className="w-urlbox" htmlFor="w-url">
            <Ico d="link" />
            <input
              id="w-url"
              ref={urlRef}
              type="text"
              value={url}
              placeholder={t(profile.placeholder)}
              onChange={(e) => setUrl(e.target.value)}
              onKeyDown={onEnter}
              spellCheck={false}
            />
            {profile.id !== "default" && <span className="w-chip">{profile.label}</span>}
          </label>
          <button type="button" id="w-paste" className="w-btn w-btn--tall" onClick={paste} title={t("cmd.pasteTitle")}>
            <Ico d="paste" />
            {t("cmd.paste")}
          </button>
          {props.updateVersion && (
            <button
              type="button"
              id="w-update"
              className="w-btn w-btn--tall w-btn--update"
              disabled={props.updateInstalling}
              title={t("cmd.updateTitle", { version: props.updateVersion })}
              onClick={props.onInstallUpdate}
            >
              {props.updateInstalling ? <span className="w-spinner" /> : <span className="w-led" aria-hidden="true" />}
              {props.updateInstalling ? t("cmd.installing") : t("cmd.update")}
            </button>
          )}
          <button type="button" id="w-download" className="w-btn w-btn--accent w-btn--tall" onClick={download}>
            <Ico d="download" />
            {t("cmd.download")}
          </button>
        </div>

        <div className="w-qhead">
          <h2>
            {t("queue.title")}
            {running > 0 && <span>{t("queue.running", { n: running })}</span>}
          </h2>
          <button type="button" id="w-clear" className="w-btn w-btn--subtle" onClick={props.onClear}>{t("queue.clear")}</button>
        </div>

        <div className="w-list">
          {tasks.length === 0 ? (
            <div className="w-empty">
              <Ico d="download" />
              <p>{t("queue.empty")}</p>
              <span>{t("queue.emptyHint")}</span>
            </div>
          ) : (
            tasks.map((t) => <TaskRow key={t.id} task={t} onCancel={props.onCancel} onOpen={props.onOpen} onRepair={props.onRepair} />)
          )}
        </div>

        {toast && <div className="w-toast" role="status">{toast}</div>}

        {ctx && (
          <div ref={ctxRef} className="w-ctx" role="menu" style={{ left: ctx.x, top: ctx.y }}>
            <button
              type="button"
              role="menuitem"
              onClick={() => {
                props.onVisualTest(true);
                setCtx(null);
              }}
            >
              <Ico d="download" />
              {t("test.fill")}
            </button>
            <div className="w-ctx__sep" />
            <button
              type="button"
              role="menuitem"
              disabled={!props.hasVisualTest}
              onClick={() => {
                props.onVisualTest(false);
                setCtx(null);
              }}
            >
              <span className="wi" />
              {t("test.clear")}
            </button>
          </div>
        )}
      </main>

      {settingsOpen && <SettingsDialog version={appVersion} onClose={() => setSettingsOpen(false)} />}
      {firstRun && <FirstRunDialog onDone={() => setFirstRun(false)} />}
    </div>
  );
}
