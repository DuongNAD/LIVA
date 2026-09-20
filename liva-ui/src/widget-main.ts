import { createApp } from "vue";
import "./style.css";
import "./uno";
import WidgetApp from "./WidgetApp.vue";

import { detectPlatform } from "./platform";
import { logger } from "./utils/logger";

const app = createApp(WidgetApp);

// Global Vue error and warning handlers to prevent unhandled widget crashes
app.config.errorHandler = (err, _instance, info) => {
  logger.error("WidgetVue", "Global widget error:", err, "info:", info);
};

app.config.warnHandler = (msg, _instance, trace) => {
  logger.warn("WidgetVue", "Warning:", msg, "trace:", trace);
};

if (typeof window !== "undefined") {
  window.addEventListener("error", (event) => {
    logger.error("WidgetWindow", "Uncaught window error:", event.error ?? event.message, event);
  });

  window.addEventListener("unhandledrejection", (event) => {
    logger.error("WidgetWindow", "Unhandled promise rejection:", event.reason);
  });
}

app.provide('platform', detectPlatform());
app.mount("#app");
