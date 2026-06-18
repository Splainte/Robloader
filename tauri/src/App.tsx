import { getCurrentWindow } from "@tauri-apps/api/window";
import "./App.css";

const appWindow = getCurrentWindow();

function App() {
  return (
    <div className="app">
      {/* Titlebar custom : la fenetre est frameless (decorations: false).
          data-tauri-drag-region rend la zone deplacable nativement. */}
      <header className="titlebar" data-tauri-drag-region>
        <span className="titlebar__title" data-tauri-drag-region>
          Robloader
        </span>

        <div className="titlebar__controls">
          <button
            className="winbtn"
            aria-label="Reduire"
            onClick={() => appWindow.minimize()}
          >
            <svg width="11" height="11" viewBox="0 0 11 11">
              <rect x="1.5" y="5" width="8" height="1" fill="currentColor" />
            </svg>
          </button>
          <button
            className="winbtn"
            aria-label="Agrandir"
            onClick={() => appWindow.toggleMaximize()}
          >
            <svg width="11" height="11" viewBox="0 0 11 11">
              <rect
                x="1.5"
                y="1.5"
                width="8"
                height="8"
                fill="none"
                stroke="currentColor"
                strokeWidth="1"
              />
            </svg>
          </button>
          <button
            className="winbtn winbtn--close"
            aria-label="Fermer"
            onClick={() => appWindow.close()}
          >
            <svg width="11" height="11" viewBox="0 0 11 11">
              <path
                d="M1.5 1.5 L9.5 9.5 M9.5 1.5 L1.5 9.5"
                stroke="currentColor"
                strokeWidth="1.1"
              />
            </svg>
          </button>
        </div>
      </header>

      <main className="content">
        <h1 className="hero">Hello, World</h1>
        <p className="subtitle">
          Fenetre native &mdash; Mica sur Windows 11, Vibrancy sur macOS.
        </p>
      </main>
    </div>
  );
}

export default App;
