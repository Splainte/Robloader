/* ============================================================
   Windows : fenetre Reglages (roue en bas du volet) et choix de la langue
   au premier lancement. Boites de dialogue Fluent (ContentDialog) posees
   sur un voile. macOS a les siennes, natives (macos_ui.rs).
   ============================================================ */

import { useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import appIcon from "./assets/app-icon.png";
import { LANGS, getLang, setLang, useLang, useT, type Lang } from "./i18n";
import { Ico } from "./WinLayout";

// Boutons radio Fluent : Francais / English.
function LanguageRadios({ value, onChange }: { value: Lang; onChange: (l: Lang) => void }) {
  const t = useT();
  return (
    <div className="w-radios" role="radiogroup" aria-label={t("settings.language")}>
      {LANGS.map((l) => (
        <label key={l.id} className="w-radio">
          <input type="radio" name="w-lang" checked={value === l.id} onChange={() => onChange(l.id)} />
          <span className="w-radio__dot" aria-hidden="true" />
          {l.label}
        </label>
      ))}
    </div>
  );
}

function Dialog({ title, children, footer, onEscape }: { title: string; children: React.ReactNode; footer: React.ReactNode; onEscape?: () => void }) {
  const box = useRef<HTMLDivElement>(null);
  useEffect(() => {
    box.current?.querySelector<HTMLElement>("input:checked, button")?.focus();
    if (!onEscape) return;
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onEscape();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onEscape]);
  return (
    <div className="w-smoke">
      <div ref={box} className="w-dialog" role="dialog" aria-modal="true" aria-label={title}>
        <div className="w-dialog__body">
          <h2 className="w-dialog__title">{title}</h2>
          {children}
        </div>
        <div className="w-dialog__foot">{footer}</div>
      </div>
    </div>
  );
}

export function SettingsDialog({ version, onClose }: { version: string; onClose: () => void }) {
  const t = useT();
  const lang = useLang();
  return (
    <Dialog
      title={t("settings.title")}
      onEscape={onClose}
      footer={
        <button type="button" className="w-btn w-btn--accent w-dialog__btn" onClick={onClose}>
          {t("settings.close")}
        </button>
      }
    >
      <div className="w-setting">
        <Ico d="globe" className="w-setting__icon" />
        <div className="w-setting__main">
          <div>
            <div>{t("settings.language")}</div>
            <div className="w-setting__hint">{t("settings.languageHint")}</div>
          </div>
          <LanguageRadios value={lang} onChange={setLang} />
        </div>
      </div>
      <div className="w-setting">
        <img className="w-setting__icon" src={appIcon} alt="" />
        <div className="w-setting__main w-setting__main--tight">
          <div>{t("settings.about")}</div>
          <div className="w-setting__hint">{t("settings.version", { version })}</div>
          <button type="button" className="w-link w-setting__link" onClick={() => invoke("open_source_page").catch(() => {})}>
            {t("settings.source")} ↗
          </button>
        </div>
      </div>
    </Dialog>
  );
}

// Premier lancement sans langue enregistree : langue du systeme preselectionnee,
// appliquee des le clic ; « Continuer » l'enregistre.
export function FirstRunDialog({ onDone }: { onDone: () => void }) {
  const t = useT();
  const lang = useLang();
  return (
    <Dialog
      title={t("welcome.title")}
      footer={
        <button
          type="button"
          className="w-btn w-btn--accent w-dialog__btn"
          onClick={() => {
            setLang(getLang());
            onDone();
          }}
        >
          {t("welcome.continue")}
        </button>
      }
    >
      <p className="w-dialog__text">{t("welcome.body")}</p>
      <LanguageRadios value={lang} onChange={setLang} />
    </Dialog>
  );
}
