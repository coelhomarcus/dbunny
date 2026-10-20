import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";
import "./index.css";
import App from "./App";
import { initI18n } from "./i18n";
import { getSettings } from "./lib/settings";

window.addEventListener("load", () => {
  getCurrentWindow().show();
});

// The language is loaded before the first render, so nothing shows in English first
initI18n(getSettings().language, navigator.languages)
  .catch(() => undefined)
  .then(() => {
    createRoot(document.getElementById("root")!).render(
      <StrictMode>
        <App />
      </StrictMode>
    );
  });
