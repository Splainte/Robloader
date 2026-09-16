/* ============================================================
   macOS : la barre d'outils et le volet de reglages sont natifs
   (src-tauri/src/macos_ui.rs). La webview n'affiche plus que la file de
   telechargements, sur fond plein, sous la barre d'outils.
   Windows garde son layout d'origine (App.tsx).
   ============================================================ */

import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Icon } from "./icons";
import type { Task } from "./App";
import "./MacLayout.css";

type Props = {
  tasks: Task[];
  onCancel: (id: number) => void;
  onOpen: (path: string) => void;
  onRepair: () => void;
  onClear: () => void;
};

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

export default function MacLayout({ tasks, onCancel, onOpen, onRepair, onClear }: Props) {
  const [toolbarH, setToolbarH] = useState(52);
  const [toast, setToast] = useState("");
  const toastTimer = useRef<number | undefined>(undefined);

  // Le contenu passe sous la barre d'outils native : on reserve sa hauteur.
  useEffect(() => {
    invoke<number>("mac_toolbar_height")
      .then((h) => h > 0 && setToolbarH(h))
      .catch(() => {});
  }, []);

  // Telecharger sans lien : message discret.
  useEffect(() => {
    const un = listen<{ url: string }>("mac://download", (e) => {
      if (e.payload.url.trim()) return;
      setToast("Colle d'abord un lien");
      window.clearTimeout(toastTimer.current);
      toastTimer.current = window.setTimeout(() => setToast(""), 2400);
    });
    return () => {
      un.then((f) => f());
    };
  }, []);

  const running = tasks.filter((t) => t.action === "cancel").length;

  return (
    <div className="app mac" data-os="macos" style={{ ["--mac-toolbar-h" as string]: `${toolbarH}px` }}>
      {/* Flou degressif sous la barre d'outils native (la webview n'a pas
          l'effet de bord de defilement d'un NSScrollView). */}
      <div className="mac-edge" aria-hidden="true" />
      <main className="mac-queue">
        <div className="mac-queue__head">
          <h2>
            File de téléchargements
            {running > 0 && <span>{running} en cours</span>}
          </h2>
          <button type="button" className="mac-pill" onClick={onClear}>
            Nettoyer la liste
          </button>
        </div>

        {tasks.length === 0 ? (
          <div className="mac-queue__empty">
            Aucun téléchargement pour l'instant. Colle un lien dans la barre d'outils.
          </div>
        ) : (
          <div className="mac-card">
            {tasks.map((t) => (
              <TaskRow key={t.id} task={t} onCancel={onCancel} onOpen={onOpen} onRepair={onRepair} />
            ))}
          </div>
        )}
      </main>

      {toast && <div className="mac-toast" role="status">{toast}</div>}
    </div>
  );
}
