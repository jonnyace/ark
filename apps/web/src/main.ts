import { mount } from "svelte";
import App from "./App.svelte";
import "./app.css";
import { isTauri } from "./lib/engine/client";

// The browser app installs a service worker that precaches every app file (offline cold start). The desktop app
// serves its files from the application bundle and does not need one.
if (!isTauri() && "serviceWorker" in navigator && location.protocol !== "file:") {
  navigator.serviceWorker.register("./sw.js", { scope: "./" }).catch((e) => console.warn("service worker:", e));
}

mount(App, { target: document.getElementById("app")! });
