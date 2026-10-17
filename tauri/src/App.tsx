import { useEffect, useMemo, useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { getVersion } from "@tauri-apps/api/app";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import "./App.css";
import MacLayout from "./MacLayout";
import WinLayout from "./WinLayout";
import { languagePickNeeded, t, useT, type Key } from "./i18n";

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
// Les libelles de qualite et de format ci-dessus servent aussi de valeurs pour
// le moteur : ils restent en francais, seul l'affichage est traduit.
export type Profile = {
  id: string;
  label: string;
  domains: string[];
  placeholder: Key;
  ladder: boolean;
  subtitles: boolean;
  thumbnail: boolean;
};
const DEFAULT_PROFILE: Profile = {
  id: "default",
  label: "Vidéo",
  domains: [],
  placeholder: "placeholder.default",
  ladder: true,
  subtitles: true,
  thumbnail: true,
};
const SITE_PROFILES: Profile[] = [
  { id: "youtube", label: "YouTube", domains: ["youtube.com", "youtu.be"], placeholder: "placeholder.youtube", ladder: true, subtitles: true, thumbnail: true },
  { id: "tiktok", label: "TikTok", domains: ["tiktok.com"], placeholder: "placeholder.tiktok", ladder: false, subtitles: false, thumbnail: true },
  { id: "instagram", label: "Instagram", domains: ["instagram.com", "instagr.am"], placeholder: "placeholder.instagram", ladder: false, subtitles: false, thumbnail: false },
  { id: "x", label: "X", domains: ["twitter.com", "x.com"], placeholder: "placeholder.x", ladder: false, subtitles: false, thumbnail: false },
  { id: "weibo", label: "Weibo", domains: ["weibo.com", "weibo.cn"], placeholder: "placeholder.weibo", ladder: false, subtitles: false, thumbnail: false },
];

// Libelles affiches (cf. Profile) : qualite et format dans la langue courante.
export function qualityDisplay(q: string): string {
  return q === QUALITY_LABELS[0] ? t("quality.max") : q;
}
export function outputDisplay(o: string): string {
  if (o === "Vidéo (natif)") return t("output.native");
  if (o === "Sous-titres seuls (.srt)") return t("output.subs");
  return o;
}

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
// Etape en cours, independante de la langue du statut (couleur de l'anneau).
export type Phase = "download" | "process";

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
  phase?: Phase;
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
  phase?: Phase;
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
    if (kind === 0) out.push({ ...base, status: `${t("test.downloading")} 0%`, percent: (i * 7) % 60 / 100, phase: "download" });
    else if (kind === 1) out.push({ ...base, status: `${t("test.converting")} 0%`, percent: (i * 5) % 50 / 100, phase: "process" });
    else if (kind === 2) out.push({ ...base, status: t("test.analyzing"), indeterminate: true });
    else if (kind === 3) out.push({ ...base, status: t("test.failed"), statusKind: "err", action: "repair" });
    else if (kind === 4) out.push({ ...base, status: t("task.cancelled"), statusKind: "warn", action: "none" });
    else out.push({ ...base, status: t("test.doneNative"), statusKind: "ok", percent: 1, action: "open", finalPath: "/tmp" });
  }
  return out;
}

// `t` est ici la tache : la traduction passe par `tr`.
const tr = t;
function tickVisualTest(t: Task): Task {
  if (t.id >= 0 || t.action !== "cancel" || t.indeterminate) return t;
  const dl = t.phase === "download";
  const p = Math.min(1, t.percent + (dl ? 0.012 : 0.007));
  if (p >= 1) {
    return dl
      ? { ...t, percent: 0, phase: "process", status: `${tr("test.converting")} 0%` }
      : { ...t, percent: 1, status: tr("test.doneHevc"), statusKind: "ok", action: "open", finalPath: "/tmp" };
  }
  const label = dl ? tr("test.downloading") : tr("test.converting");
  return { ...t, percent: p, status: `${label} ${Math.round(p * 100)}%` };
}

function App() {
  // Re-rendu au changement de langue (Reglages).
  const t = useT();
  const [url, setUrl] = useState("");
  const [quality, setQuality] = useState(QUALITY_LABELS[0]);
  const [transcode, setTranscode] = useState(true);
  const [output, setOutput] = useState(OUTPUTS_TRANSCODE[0]);
  const [subs, setSubs] = useState(false);
  const [thumb, setThumb] = useState(false);
  const [start, setStart] = useState("");
  const [end, setEnd] = useState("");
  // Windows : l'extrait est un interrupteur ; coupe, Debut/Fin sont ignores.
  const [clip, setClip] = useState(false);

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
        appWindow.show().then(() => {
          // macOS : 1er lancement sans langue choisie (pas d'installeur) ->
          // feuille native sur la fenetre, une fois celle-ci visible.
          if (isMac && languagePickNeeded()) invoke("mac_first_run").catch(() => {});
        }).catch(() => {});
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
                ...(u.phase !== undefined ? { phase: u.phase } : {}),
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
    visualTest: (_fill: boolean) => {},
  });
  macHandlers.current = {
    download: startDownloadWith,
    chooseDestination,
    revealDestination: () => env && openFolder(env.downloadDir),
    repair,
    installUpdate,
    visualTest,
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
      listen<boolean>("mac://visual-test", (e) => macHandlers.current.visualTest(e.payload)),
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
      start: clip ? start : "",
      end: clip ? end : "",
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
        title: t("task.analyzing"),
        status: t("task.waiting"),
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
              ? { ...t, status: tr("task.failed", { error: String(err) }), statusKind: "err", action: "none", indeterminate: false }
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

  // Windows : Ctrl+Maj+T remplit / vide la file factice (equivalent du menu
  // « Test visuel » de macOS).
  useEffect(() => {
    if (isMac) return;
    const onKey = (e: KeyboardEvent) => {
      if (!(e.ctrlKey && e.shiftKey && e.key.toLowerCase() === "t")) return;
      e.preventDefault();
      macHandlers.current.visualTest(!visualTestOn.current);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

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
          t.id === id ? { ...t, status: tr("task.cancelled"), statusKind: "warn", action: "none", indeterminate: false } : t
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
  // Test visuel : file factice + bouton « Mise a jour » factice (url vide,
  // un clic ne fait rien) si aucune vraie mise a jour n'est disponible.
  const visualTestOn = useRef(false);
  function visualTest(fill: boolean) {
    visualTestOn.current = fill;
    setTasks((prev) => [...(fill ? makeVisualTestTasks() : []), ...prev.filter((t) => t.id > 0)]);
    setAvailableUpdate((prev) => {
      if (fill) return prev ?? { version: "test", url: "" };
      return prev?.url === "" ? null : prev;
    });
  }
  async function installUpdate() {
    if (!availableUpdate?.url) return;
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
    <WinLayout
      url={url}
      setUrl={setUrl}
      profile={profile}
      qualities={QUALITY_LABELS}
      quality={quality}
      setQuality={setQuality}
      clip={clip}
      setClip={setClip}
      start={start}
      setStart={setStart}
      end={end}
      setEnd={setEnd}
      transcode={transcode}
      toggleTranscode={toggleTranscode}
      output={output}
      setOutput={setOutput}
      outputs={outputs}
      subs={subs}
      setSubs={setSubs}
      thumb={thumb}
      setThumb={setThumb}
      tasks={tasks}
      env={env}
      appVersion={appVersion}
      updateVersion={availableUpdate?.version ?? null}
      updateInstalling={updateInstalling}
      onInstallUpdate={installUpdate}
      onDownload={startDownload}
      onCancel={cancelTask}
      onOpen={openFolder}
      onRepair={repair}
      onClear={clearList}
      onChooseDestination={chooseDestination}
      hasVisualTest={tasks.some((t) => t.id < 0)}
      onVisualTest={visualTest}
    />
  );
}

export default App;
