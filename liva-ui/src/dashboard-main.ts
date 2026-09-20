import { createApp } from "vue";
import "./dashboard.css";
import "./uno";
import DashboardApp from "./DashboardApp.vue";

import { detectPlatform } from "./platform";
import { logger } from "./utils/logger";

const app = createApp(DashboardApp);

// Global Vue error and warning handlers to prevent unhandled dashboard crashes
app.config.errorHandler = (err, _instance, info) => {
  logger.error("DashboardVue", "Global dashboard error:", err, "info:", info);
};

app.config.warnHandler = (msg, _instance, trace) => {
  logger.warn("DashboardVue", "Warning:", msg, "trace:", trace);
};

if (typeof window !== "undefined") {
  window.addEventListener("error", (event) => {
    logger.error("DashboardWindow", "Uncaught window error:", event.error ?? event.message, event);
  });

  window.addEventListener("unhandledrejection", (event) => {
    logger.error("DashboardWindow", "Unhandled promise rejection:", event.reason);
  });
}

app.provide('platform', detectPlatform());
app.mount("#app");
