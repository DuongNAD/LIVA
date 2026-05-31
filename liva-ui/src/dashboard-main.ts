import { createApp } from "vue";
import "./dashboard.css";
import "virtual:uno.css";
import DashboardApp from "./DashboardApp.vue";

import { detectPlatform } from "./platform";

const app = createApp(DashboardApp);
app.provide('platform', detectPlatform());
app.mount("#app");
