import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { initLang } from "./i18n";
import "./styles.css";

// La langue (tenue cote Rust) est connue avant le premier rendu.
initLang().then(() => {
  ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <React.StrictMode>
      <App />
    </React.StrictMode>,
  );
});
