import { createApp } from "vue";
import "./dashboard.css";
import "virtual:uno.css";
import DashboardApp from "./DashboardApp.vue";

import { TauriAdapter } from "./platform/TauriAdapter";

const app = createApp(DashboardApp);
app.provide('platform', new TauriAdapter());
app.mount("#app");
