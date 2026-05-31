import { createApp } from "vue";
import "./style.css";
import "virtual:uno.css";
import WidgetApp from "./WidgetApp.vue";

import { detectPlatform } from "./platform";

const app = createApp(WidgetApp);
app.provide('platform', detectPlatform());
app.mount("#app");
