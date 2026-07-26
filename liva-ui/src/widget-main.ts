import { createApp } from "vue";
import "./style.css";
import "./uno";
import WidgetApp from "./WidgetApp.vue";

import { detectPlatform } from "./platform";

const app = createApp(WidgetApp);
app.provide('platform', detectPlatform());
app.mount("#app");
