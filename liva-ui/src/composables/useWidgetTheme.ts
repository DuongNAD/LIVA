import { ref } from "vue";

export function useWidgetTheme() {
  const isLightMode = ref(globalThis.localStorage?.getItem("theme") === "light");

  const applyTheme = (theme: "light" | "dark") => {
    globalThis.document?.documentElement.setAttribute("data-theme", theme);
    globalThis.document?.body.setAttribute("data-theme", theme);
  };

  const toggleTheme = () => {
    isLightMode.value = !isLightMode.value;
    const newTheme = isLightMode.value ? "light" : "dark";
    applyTheme(newTheme);
    globalThis.localStorage?.setItem("theme", newTheme);
  };

  const initTheme = () => {
    const initialTheme = isLightMode.value ? "light" : "dark";
    applyTheme(initialTheme);
  };

  return {
    isLightMode,
    toggleTheme,
    initTheme,
  };
}
