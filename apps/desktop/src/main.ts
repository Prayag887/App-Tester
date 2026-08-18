import { mount } from "svelte";
import { invoke } from "@tauri-apps/api/core";
import App from "./App.svelte";
import "./styles.css";
import "./motion.css";
import { initializeTheme, waitForThemeStyles } from "./theme";

initializeTheme();
mount(App, { target: document.getElementById("root")! });

// Hidden WebViews may throttle requestAnimationFrame. Wait for the selected
// palette chunk instead, then let the native compositor reveal the mounted UI.
if ("__TAURI_INTERNALS__" in window) {
  void waitForThemeStyles()
    .then(() => invoke("frontend_ready"))
    .catch(() => invoke("frontend_ready"))
    .catch(() => undefined);
}
