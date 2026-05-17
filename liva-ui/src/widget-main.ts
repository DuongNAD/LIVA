import { createApp } from "vue";
import "./style.css";
import "virtual:uno.css";
import WidgetApp from "./WidgetApp.vue";

import { TauriAdapter } from "./platform/TauriAdapter";

const app = createApp(WidgetApp);
app.provide('platform', new TauriAdapter());
app.mount("#app");
