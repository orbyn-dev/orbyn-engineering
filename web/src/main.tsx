import "@fontsource-variable/geist/wght.css";
import "@fontsource/jetbrains-mono/400.css";
import "@fontsource/jetbrains-mono/500.css";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { captureToken } from "./api/token";
import { App } from "./app/App";
import { trackInputModality } from "./lib/inputModality";
import { applyTheme, useUi } from "./product/state/ui";
import "./styles/tokens.css";
import "./styles/base.css";
import "./styles/shell.css";
import "./styles/thread.css";
import "./styles/thread-ask.css";
import "./styles/composer.css";
import "./styles/thread-panels.css";
import "./styles/model-picker.css";
import "./styles/thread-home.css";
import "./styles/documents.css";
import "./styles/rich-content.css";
import "./styles/thread-overlays.css";
import "./styles/diff.css";
import "./styles/changes.css";
import "./styles/browser.css";
import "./styles/toolcalls.css";
import "./styles/detailsCard.css";
import "./styles/tools.css";
import "./styles/panelTabs.css";
import "./styles/panelStrip.css";
import "./styles/explorer.css";
import "./styles/editor.css";
import "./styles/pages.css";
import "./styles/layouts.css";
import "./styles/selection.css";

captureToken();
applyTheme(useUi.getState().theme);
trackInputModality();

const root = document.getElementById("root");
if (root) {
  createRoot(root).render(
    <StrictMode>
      <App />
    </StrictMode>,
  );
}

// throwaway scope proof
