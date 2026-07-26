import { createApp } from "vue";
import "./style.css";
import "./uno";
import App from "./App.vue";

import { detectPlatform } from "./platform";

const app = createApp(App);

// [Phase 5.1] Tự động detect môi trường (Tauri/Web) và inject adapter
const platform = detectPlatform();
app.provide('platform', platform);

app.mount("#app");
