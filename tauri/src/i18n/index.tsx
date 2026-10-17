// ============================================================
//  Robloader — traduction de l'interface
// ============================================================
//
// Pas de bibliotheque : un dictionnaire par langue (fr.ts, en.ts) et quelques
// fonctions. La langue est tenue cote Rust (src-tauri/src/i18n.rs) : elle doit
// exister avant le front pour l'interface native macOS.
//
//   t("queue.clear")                 texte simple
//   t("queue.running", { n: 2 })     avec variables : « {n} » dans le texte
//
// Les composants appellent `useT()` : il les abonne au changement de langue
// (Reglages) et ils se re-rendent dans la nouvelle langue sans redemarrer.
//
// Ajouter une langue : copier en.ts, traduire, l'ajouter a DICTS et LANGS
// (et cote Rust). TypeScript refuse une cle manquante.

import { useSyncExternalStore } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { fr } from "./fr";
import { en } from "./en";

export type Lang = "fr" | "en";
export type Key = keyof typeof fr;
type Vars = Record<string, string | number>;

const DICTS: Record<Lang, Record<Key, string>> = { fr, en };

// Nom de chaque langue DANS cette langue (on la reconnait sans la parler).
export const LANGS: { id: Lang; label: string }[] = [
  { id: "fr", label: "Français" },
  { id: "en", label: "English" },
];

let current: Lang = "fr";
let pickNeeded = false;
const listeners = new Set<() => void>();

export const getLang = () => current;
export const languagePickNeeded = () => pickNeeded;

export function t(key: Key, vars?: Vars): string {
  const s = DICTS[current][key] ?? fr[key] ?? key;
  if (!vars) return s;
  return s.replace(/\{(\w+)\}/g, (m, k: string) => (k in vars ? String(vars[k]) : m));
}

function apply(lang: Lang) {
  pickNeeded = false;
  if (lang === current) return;
  current = lang;
  document.documentElement.lang = lang;
  listeners.forEach((f) => f());
}

// Le changement est confirme par l'evenement « app://language » (cf. initLang),
// le meme que celui de la fenetre Reglages native de macOS.
export function setLang(lang: Lang): Promise<void> {
  apply(lang);
  return invoke<void>("set_language", { lang }).catch(() => {});
}

function subscribe(f: () => void) {
  listeners.add(f);
  return () => listeners.delete(f);
}

// Langue courante, en abonnant le composant a ses changements.
export function useLang(): Lang {
  return useSyncExternalStore(subscribe, getLang);
}

// A appeler dans tout composant qui affiche du texte : re-rend le composant
// quand la langue change. Renvoie `t` pour l'ecriture `const t = useT()`.
export function useT() {
  useLang();
  return t;
}

// Avant le premier rendu.
export async function initLang() {
  const info = await invoke<{ lang: Lang; pickNeeded: boolean }>("get_language").catch(() => null);
  if (info) {
    current = info.lang === "en" ? "en" : "fr";
    pickNeeded = info.pickNeeded;
  }
  document.documentElement.lang = current;
  listen<string>("app://language", (e) => apply(e.payload === "en" ? "en" : "fr")).catch(() => {});
}
