import { mount } from "svelte";
import App from "./App.svelte";
import "./styles.css";
import "./motion.css";
import { initializeTheme } from "./theme";

initializeTheme();
mount(App, { target: document.getElementById("root")! });

// The dependency-free splash in index.html covers WebView/module startup.
// Dismiss it only after Svelte has mounted and the browser has painted once.
window.requestAnimationFrame(() => window.requestAnimationFrame(() => {
  const splash = document.getElementById("startup-splash");
  if (!splash) return;
  splash.classList.add("leaving");
  window.setTimeout(() => splash.remove(), 520);
}));
