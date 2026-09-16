/* ============================================================
   Layout macOS « D3 » (etape 1 du redesign, cf. docs/redesign-macos) :
   barre d'outils en haut, volet de reglages toujours ouvert a gauche,
   file de telechargements a droite sur fond plein.
   Tout est encore en HTML : la barre native (NSToolbar), le verre sous
   le volet et les menus natifs arrivent aux etapes suivantes.
   Windows garde son layout d'origine (App.tsx).
   ============================================================ */

import { useEffect, useRef, useState, type KeyboardEvent as ReactKeyboardEvent } from "react";
import { Icon } from "./icons";
import type { EnvInfo, Profile, Task } from "./App";
import "./MacLayout.css";

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
};

// Libelle court de la grille de qualite : « Qualité max (jusqu'à 4K) » -> « Max ».
function shortQuality(q: string) {
  return q.startsWith("Qualité max") ? "Max" : q.replace(/ \(.*\)$/, "");
}

// ~/Movies/Robloader plutot que /Users/robin/Movies/Robloader, tronque au milieu.
function prettyPath(p: string) {
  const home = p.replace(/^\/Users\/[^/]+/, "~");
  if (home.length <= 34) return home;
  return home.slice(0, 13) + "…" + home.slice(-20);
}

function Switch({
  checked,
  onChange,
  label,
}: {
  checked: boolean;
  onChange: (v: boolean) => void;
  label: string;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      className="mac-switch"
      onClick={() => onChange(!checked)}
    />
  );
}

// Anneau de progression : bleu d'accent au telechargement, violet au transcodage.
const RING_C = 2 * Math.PI * 8;
function Ring({ task }: { task: Task }) {
  const tc = !task.indeterminate && !task.status.startsWith("Téléchargement");
  const p = task.indeterminate ? 0.28 : task.percent;
  return (
    <svg
      className={`mac-ring${task.indeterminate ? " is-spinning" : ""}${tc ? " is-transcode" : ""}`}
      viewBox="0 0 20 20"
      aria-hidden="true"
    >
      <circle className="mac-ring__track" cx="10" cy="10" r="8" />
      <circle
        className="mac-ring__fill"
        cx="10"
        cy="10"
        r="8"
        strokeDasharray={RING_C}
        strokeDashoffset={RING_C * (1 - p)}
      />
    </svg>
  );
}

function TaskRow({
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
  const [thumbOk, setThumbOk] = useState(true);
  return (
    <div className="mac-task">
      <div className="mac-task__thumb">
        {task.thumbnail && thumbOk ? (
          <img src={task.thumbnail} alt="" onError={() => setThumbOk(false)} />
        ) : (
          <Icon name="download" className="mac-task__thumb-icon" />
        )}
      </div>
      <div className="mac-task__text">
        <div className="mac-task__title" title={task.title}>
          {task.title}
        </div>
        <div className={`mac-task__status mac-task__status--${task.statusKind}`} title={task.status}>
          {task.status}
        </div>
      </div>
      <div className="mac-task__action">
        {task.action === "cancel" && (
          <>
            <Ring task={task} />
            <button type="button" className="mac-pill" onClick={() => onCancel(task.id)}>
              Annuler
            </button>
          </>
        )}
        {task.action === "open" && (
          <button
            type="button"
            className="mac-pill"
            onClick={() => task.finalPath && onOpen(task.finalPath)}
          >
            Afficher
          </button>
        )}
        {task.action === "repair" && (
          <button type="button" className="mac-pill mac-pill--accent" onClick={onRepair}>
            Réparer
          </button>
        )}
      </div>
    </div>
  );
}

export default function MacLayout(props: Props) {
  const {
    url, setUrl, profile, qualities, quality, setQuality,
    clip, setClip, start, setStart, end, setEnd,
    transcode, toggleTranscode, output, setOutput, outputs,
    subs, setSubs, thumb, setThumb, tasks, env, appVersion,
  } = props;

  const urlRef = useRef<HTMLInputElement>(null);
  const [toast, setToast] = useState("");
  const toastTimer = useRef<number | undefined>(undefined);

  function showToast(msg: string) {
    setToast(msg);
    window.clearTimeout(toastTimer.current);
    toastTimer.current = window.setTimeout(() => setToast(""), 2400);
  }

  function download() {
    if (!url.trim()) {
      showToast("Colle d'abord un lien");
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
      showToast("Colle le lien avec ⌘V");
    }
  }

  // ⌘L : focus dans le champ de lien.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.metaKey && !e.shiftKey && !e.altKey && e.key.toLowerCase() === "l") {
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

  return (
    <div className="app mac" data-os="macos">
      {/* Fond plein partout SAUF sous le volet (ombre portee geante), pour
          laisser voir le materiau natif derriere le volet uniquement. */}
      <div className="mac-backdrop" aria-hidden="true" />

      {/* ---- Barre d'outils ---- */}
      <header className="mac-toolbar" data-tauri-drag-region>
        <div className="mac-toolbar__title" data-tauri-drag-region>
          Robloader
          {appVersion && <span data-tauri-drag-region>{appVersion}</span>}
        </div>

        <label className="mac-url">
          <Icon name="link" className="mac-url__icon" />
          <input
            ref={urlRef}
            type="text"
            value={url}
            placeholder={profile.placeholder}
            aria-label="Lien de la vidéo"
            onChange={(e) => setUrl(e.target.value)}
            onKeyDown={onEnter}
            spellCheck={false}
          />
          {profile.id !== "default" && <span className="mac-chip">{profile.label}</span>}
          <button type="button" className="mac-url__paste" aria-label="Coller le lien" title="Coller" onClick={paste}>
            <Icon name="paste" />
          </button>
        </label>

        {props.updateVersion && (
          <button
            type="button"
            className="mac-tbtn"
            title={`Installer la version ${props.updateVersion} et relancer`}
            disabled={props.updateInstalling}
            onClick={props.onInstallUpdate}
          >
            {props.updateInstalling ? <span className="mac-spinner" /> : <span className="mac-badge" />}
            {props.updateInstalling ? "Installation…" : "Mise à jour"}
          </button>
        )}

        <button type="button" className="mac-download" onClick={download}>
          <Icon name="download" />
          Télécharger
        </button>
      </header>

      {/* ---- Volet de reglages (toujours ouvert) ---- */}
      <aside className="mac-sidebar" aria-label="Réglages du téléchargement">
        {profile.ladder && (
          <section>
            <h4>Qualité</h4>
            <div className="mac-qgrid">
              {qualities.map((q) => (
                <button
                  type="button"
                  key={q}
                  title={q}
                  aria-pressed={q === quality}
                  className="mac-qopt"
                  onClick={() => setQuality(q)}
                >
                  {shortQuality(q)}
                </button>
              ))}
            </div>
          </section>
        )}

        <section>
          <h4>Options</h4>
          <div className="mac-group">
            <div className="mac-row">
              <span className="mac-row__label">Extraire un passage</span>
              <Switch checked={clip} onChange={setClip} label="Extraire un passage" />
            </div>
            <div className={`mac-reveal${clip ? " is-open" : ""}`} aria-hidden={!clip}>
              <div className="mac-row mac-row--times">
                <span className="mac-row__sub">Début</span>
                <input
                  className="mac-time"
                  value={start}
                  placeholder="00:00"
                  aria-label="Début"
                  tabIndex={clip ? 0 : -1}
                  onChange={(e) => setStart(e.target.value)}
                  onKeyDown={onEnter}
                  spellCheck={false}
                />
                <span className="mac-row__sub">Fin</span>
                <input
                  className="mac-time"
                  value={end}
                  placeholder="01:30"
                  aria-label="Fin"
                  tabIndex={clip ? 0 : -1}
                  onChange={(e) => setEnd(e.target.value)}
                  onKeyDown={onEnter}
                  spellCheck={false}
                />
              </div>
            </div>

            <div className="mac-row">
              <span className="mac-row__label">Transcodage</span>
              <Switch checked={transcode} onChange={toggleTranscode} label="Transcodage" />
            </div>
            <div className={`mac-reveal${transcode ? " is-open" : ""}`} aria-hidden={!transcode}>
              <div className="mac-row">
                <span className="mac-row__label">Format de sortie</span>
                <select
                  className="mac-popup"
                  value={output}
                  aria-label="Format de sortie"
                  tabIndex={transcode ? 0 : -1}
                  onChange={(e) => setOutput(e.target.value)}
                >
                  {outputs.map((o) => (
                    <option key={o} value={o}>
                      {o}
                    </option>
                  ))}
                </select>
              </div>
            </div>

            {profile.subtitles && (
              <div className="mac-row">
                <span className="mac-row__label">Sous-titres (.srt)</span>
                <Switch checked={subs} onChange={setSubs} label="Sous-titres" />
              </div>
            )}
            {profile.thumbnail && (
              <div className="mac-row">
                <span className="mac-row__label">Miniature</span>
                <Switch checked={thumb} onChange={setThumb} label="Miniature" />
              </div>
            )}
          </div>
        </section>

        <section>
          <h4>Destination</h4>
          <div className="mac-group">
            <div className="mac-row">
              <span className="mac-row__label mac-row__path" title={env?.downloadDir}>
                {env ? prettyPath(env.downloadDir) : "…"}
              </span>
            </div>
            <div className="mac-row mac-row--buttons">
              <button type="button" className="mac-pill" onClick={props.onChooseDestination}>
                Choisir…
              </button>
              <button
                type="button"
                className="mac-pill"
                disabled={!env}
                onClick={() => env && props.onOpen(env.downloadDir)}
              >
                Afficher
              </button>
            </div>
          </div>
        </section>

        {env && (
          <footer className="mac-sidebar__foot">
            {env.cookiesOk ? (
              <span>
                <i className="mac-dot mac-dot--on" />
                cookies {env.cookiesSource} ✓
              </span>
            ) : (
              <button type="button" className="mac-link" onClick={props.onRepair}>
                <i className="mac-dot" />
                cookies absents
              </button>
            )}
            {!env.jsRuntime && <span>4K limitée (Deno absent)</span>}
          </footer>
        )}
      </aside>

      {/* ---- File de telechargements ---- */}
      <main className="mac-queue">
        <div className="mac-queue__head">
          <h2>
            File de téléchargements
            {running > 0 && <span>{running} en cours</span>}
          </h2>
          <button type="button" className="mac-pill" onClick={props.onClear}>
            Nettoyer la liste
          </button>
        </div>

        {tasks.length === 0 ? (
          <div className="mac-queue__empty">
            Aucun téléchargement pour l'instant. Colle un lien dans la barre d'outils.
          </div>
        ) : (
          <div className="mac-group mac-group--card">
            {tasks.map((t) => (
              <TaskRow
                key={t.id}
                task={t}
                onCancel={props.onCancel}
                onOpen={props.onOpen}
                onRepair={props.onRepair}
              />
            ))}
          </div>
        )}
      </main>

      {toast && <div className="mac-toast" role="status">{toast}</div>}
    </div>
  );
}
