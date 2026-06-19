/* ============================================================
   Jeu d'icones unique, en trait (stroke = currentColor).
   L'aspect "SF Symbols" (macOS) vs "Fluent" (Windows) est pilote
   en CSS via .icon : caps arrondis + trait un peu plus epais sur
   macOS, caps droits + trait plus fin facon Fluent sur Windows.
   On garde donc un seul SVG par symbole.
   ============================================================ */

import type { ReactElement } from "react";

type IconName =
  | "link"
  | "folder"
  | "folder-open"
  | "download"
  | "scissors"
  | "check"
  | "chevron"
  | "broom"
  | "tray"
  | "x"
  | "wrench";

const PATHS: Record<IconName, ReactElement> = {
  link: (
    <>
      <path d="M9 14a4 4 0 0 0 6 0l3-3a4 4 0 0 0-6-6l-1.5 1.5" />
      <path d="M15 10a4 4 0 0 0-6 0l-3 3a4 4 0 0 0 6 6l1.5-1.5" />
    </>
  ),
  folder: (
    <path d="M4 7a2 2 0 0 1 2-2h3l2 2h7a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2z" />
  ),
  "folder-open": (
    <path d="M4 8a2 2 0 0 1 2-2h3l2 2h7a2 2 0 0 1 2 2v1H7l-2 7H5a1 1 0 0 1-1-1zM7 11h14l-2.2 7H5z" />
  ),
  download: (
    <>
      <path d="M12 4v11" />
      <path d="M8 11l4 4 4-4" />
      <path d="M5 19h14" />
    </>
  ),
  scissors: (
    <>
      <circle cx="6.5" cy="7" r="2.2" />
      <circle cx="6.5" cy="17" r="2.2" />
      <path d="M8.4 8.4 19 18M8.4 15.6 19 6" />
    </>
  ),
  check: <path d="M5 12.5l4 4 10-10" />,
  chevron: <path d="M6 9l6 6 6-6" />,
  broom: (
    <>
      <path d="M14 4 9 11" />
      <path d="M5 20c0-3 1.5-5 4-6l6 3c-1 2.5-3 4-6 4z" />
      <path d="M11 18l4-2" />
    </>
  ),
  tray: (
    <>
      <path d="M4 13l2.5-7h11L20 13v4a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2z" />
      <path d="M4 13h4l1.5 2h5L16 13h4" />
    </>
  ),
  x: <path d="M6 6l12 12M18 6L6 18" />,
  wrench: (
    <path d="M14.7 6.3a4 4 0 0 0-5.2 5l-5.1 5.1a1.5 1.5 0 0 0 2.1 2.1l5.1-5.1a4 4 0 0 0 5-5.2l-2.4 2.4-2.1-.6-.6-2.1z" />
  ),
};

export function Icon({
  name,
  className,
}: {
  name: IconName;
  className?: string;
}) {
  return (
    <svg
      className={`icon ${className ?? ""}`}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      aria-hidden="true"
    >
      {PATHS[name]}
    </svg>
  );
}
