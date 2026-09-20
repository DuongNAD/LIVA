import { createApp } from "vue";
import "./style.css";
import "./uno";
import App from "./App.vue";

import { detectPlatform } from "./platform";
import { logger } from "./utils/logger";

const app = createApp(App);

// Global Vue error and warning handlers to prevent unhandled crashes
app.config.errorHandler = (err, _instance, info) => {
  logger.error("Vue", "Global app error:", err, "info:", info);
};

app.config.warnHandler = (msg, _instance, trace) => {
  logger.warn("Vue", "Warning:", msg, "trace:", trace);
};

if (typeof window !== "undefined") {
  window.addEventListener("error", (event) => {
    logger.error("Window", "Uncaught window error:", event.error ?? event.message, event);
  });

  window.addEventListener("unhandledrejection", (event) => {
    logger.error("Window", "Unhandled promise rejection:", event.reason);
  });
}

// [Phase 5.1] Tự động detect môi trường (Tauri/Web) và inject adapter
const platform = detectPlatform();
app.provide('platform', platform);

app.mount("#app");
