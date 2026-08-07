<script setup lang="ts">
import { computed, onBeforeUnmount, watch } from "vue";

const props = defineProps<{
  tool: string;
  state: "loading" | "done" | "error";
  payload: unknown;
}>();

const emit = defineEmits<{
  close: [];
}>();

let autoCloseTimer: ReturnType<typeof setTimeout> | null = null;

const clearAutoClose = () => {
  if (autoCloseTimer) clearTimeout(autoCloseTimer);
  autoCloseTimer = null;
};

watch(() => props.state, (state) => {
  clearAutoClose();
  if (state === "done") {
    autoCloseTimer = setTimeout(() => emit("close"), 10_000);
  }
}, { immediate: true });

onBeforeUnmount(clearAutoClose);

const isRecord = (value: unknown): value is Record<string, unknown> => (
  typeof value === "object" && value !== null && !Array.isArray(value)
);

const toolText = computed(() => {
  if (typeof props.payload === "string") return props.payload.trim();
  if (!isRecord(props.payload)) return "";
  if (typeof props.payload.text === "string") return props.payload.text.trim();
  if (!Array.isArray(props.payload.content)) return "";

  for (const item of props.payload.content) {
    if (isRecord(item) && typeof item.text === "string" && item.text.trim()) {
      return item.text.trim();
    }
  }
  return "";
});

const weatherIcon = (description: string) => {
  const normalized = description.toLocaleLowerCase("vi");
  if (normalized.includes("dông")) return "⛈️";
  if (normalized.includes("mưa")) return "🌧️";
  if (normalized.includes("tuyết")) return "🌨️";
  if (normalized.includes("sương")) return "🌫️";
  if (normalized.includes("quang")) return "☀️";
  return "🌤️";
};

const weather = computed(() => {
  const data = isRecord(props.payload) ? props.payload : {};
  const parsed = toolText.value.match(
    /^(.+?):\s*(-?\d+(?:[.,]\d+)?)°C,\s*([^,.]+)(?:,\s*độ ẩm\s*(\d+(?:[.,]\d+)?)%)?\.?$/i,
  );
  const parsedDescription = parsed?.[3]?.trim() ?? "";

  return {
    location: typeof data.location === "string" ? data.location : (parsed?.[1]?.trim() ?? "Thời tiết hiện tại"),
    icon: typeof data.icon === "string" ? data.icon : weatherIcon(parsedDescription),
    temperature: typeof data.temperature === "number"
      ? data.temperature
      : parsed?.[2]
        ? Number(parsed[2].replace(",", "."))
        : null,
    description: typeof data.description === "string"
      ? data.description
      : parsedDescription || "Chưa có mô tả",
  };
});

const errorMessage = computed(() => {
  if (typeof props.payload === "string" && props.payload.trim()) return props.payload;
  if (isRecord(props.payload)) {
    for (const key of ["message", "error", "reason"] as const) {
      const value = props.payload[key];
      if (typeof value === "string" && value.trim()) return value;
    }
  }
  return "Công cụ không hoàn tất. Vui lòng thử lại.";
});

const loadingLabel = computed(() => {
  if (isRecord(props.payload) && typeof props.payload.label === "string" && props.payload.label.trim()) {
    return props.payload.label;
  }
  return `Đang xử lý ${props.tool}…`;
});

const formattedPayload = computed(() => {
  if (props.payload === null || props.payload === undefined) return "Không có dữ liệu trả về.";
  if (typeof props.payload === "string") return props.payload;
  try {
    return JSON.stringify(props.payload, null, 2);
  } catch {
    return String(props.payload);
  }
});
</script>

<template>
  <section
    class="tool-panel"
    role="status"
    aria-live="polite"
    data-placement="left-bottom"
  >
    <button
      class="tool-panel__close"
      type="button"
      aria-label="Đóng bảng công cụ"
      @click="emit('close')"
    >
      ×
    </button>
    <div v-if="state === 'loading'" class="tool-panel__loading">
      <span class="tool-panel__spinner" aria-hidden="true" />
      <span>{{ loadingLabel }}</span>
    </div>
    <div v-else-if="state === 'error'" class="tool-panel__error" role="alert">
      <strong>Không thể hoàn tất</strong>
      <span>{{ errorMessage }}</span>
    </div>
    <div v-else-if="tool === 'get_weather'" class="tool-panel__weather">
      <span class="tool-panel__eyebrow">{{ weather.location }}</span>
      <span class="tool-panel__weather-icon" aria-hidden="true">{{ weather.icon }}</span>
      <strong class="tool-panel__temperature">
        {{ weather.temperature === null ? "—" : `${weather.temperature}°C` }}
      </strong>
      <span class="tool-panel__description">{{ weather.description }}</span>
    </div>
    <div v-else class="tool-panel__default">
      <strong>{{ tool }}</strong>
      <pre>{{ formattedPayload }}</pre>
    </div>
  </section>
</template>

<style scoped>
.tool-panel {
  box-sizing: border-box;
  position: relative;
  width: min(340px, calc(100vw - 48px));
  min-height: 116px;
  padding: 18px 44px 18px 20px;
  overflow: hidden;
  color: #f8fafc;
  background:
    radial-gradient(circle at 12% 0%, rgba(167, 139, 250, 0.18), transparent 42%),
    rgba(15, 23, 42, 0.9);
  border: 1px solid rgba(196, 181, 253, 0.24);
  border-radius: 20px;
  box-shadow: 0 18px 50px rgba(2, 6, 23, 0.4), inset 0 1px 0 rgba(255, 255, 255, 0.08);
  backdrop-filter: blur(18px) saturate(130%);
  -webkit-backdrop-filter: blur(18px) saturate(130%);
  pointer-events: auto;
}

.tool-panel__close {
  position: absolute;
  top: 10px;
  right: 10px;
  display: grid;
  width: 28px;
  height: 28px;
  padding: 0;
  place-items: center;
  color: rgba(226, 232, 240, 0.72);
  font: 500 20px/1 system-ui, sans-serif;
  background: rgba(255, 255, 255, 0.06);
  border: 1px solid rgba(255, 255, 255, 0.08);
  border-radius: 999px;
  cursor: pointer;
  transition: color 150ms ease, background 150ms ease, transform 150ms ease;
}

.tool-panel__close:hover {
  color: #fff;
  background: rgba(255, 255, 255, 0.14);
  transform: scale(1.06);
}

.tool-panel__close:focus-visible {
  outline: 2px solid #a78bfa;
  outline-offset: 2px;
}

.tool-panel__loading,
.tool-panel__error {
  display: flex;
  min-height: 80px;
  align-items: center;
  gap: 12px;
  font: 500 14px/1.5 system-ui, sans-serif;
}

.tool-panel__error {
  align-items: flex-start;
  flex-direction: column;
  justify-content: center;
  gap: 4px;
  color: #fecaca;
}

.tool-panel__error strong {
  color: #fca5a5;
  font-size: 14px;
}

.tool-panel__error span {
  color: rgba(254, 226, 226, 0.82);
  font-size: 13px;
}

.tool-panel__weather {
  display: grid;
  grid-template-columns: auto 1fr;
  grid-template-areas:
    "eyebrow eyebrow"
    "icon temperature"
    "icon description";
  align-items: center;
  column-gap: 14px;
}

.tool-panel__eyebrow {
  grid-area: eyebrow;
  margin-bottom: 6px;
  overflow: hidden;
  color: rgba(226, 232, 240, 0.7);
  font: 600 11px/1.4 system-ui, sans-serif;
  letter-spacing: 0.08em;
  text-overflow: ellipsis;
  text-transform: uppercase;
  white-space: nowrap;
}

.tool-panel__weather-icon {
  grid-area: icon;
  font-size: 42px;
  filter: drop-shadow(0 8px 12px rgba(15, 23, 42, 0.45));
}

.tool-panel__temperature {
  grid-area: temperature;
  font: 650 34px/1.05 system-ui, sans-serif;
  letter-spacing: -0.04em;
}

.tool-panel__description {
  grid-area: description;
  color: rgba(226, 232, 240, 0.78);
  font: 500 13px/1.4 system-ui, sans-serif;
}

.tool-panel__default {
  display: flex;
  min-width: 0;
  flex-direction: column;
  gap: 8px;
  font-family: system-ui, sans-serif;
}

.tool-panel__default strong {
  overflow: hidden;
  color: #ddd6fe;
  font-size: 13px;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.tool-panel__default pre {
  max-height: 190px;
  margin: 0;
  padding: 10px 12px;
  overflow: auto;
  color: rgba(226, 232, 240, 0.82);
  background: rgba(2, 6, 23, 0.32);
  border-radius: 10px;
  font: 11px/1.5 ui-monospace, SFMono-Regular, Consolas, monospace;
  overflow-wrap: anywhere;
  white-space: pre-wrap;
}

.tool-panel__spinner {
  display: inline-block;
  width: 18px;
  height: 18px;
  border: 2px solid rgba(255, 255, 255, 0.22);
  border-top-color: #a78bfa;
  border-radius: 50%;
  animation: tool-panel-spin 0.8s linear infinite;
}

@keyframes tool-panel-spin {
  to { transform: rotate(360deg); }
}

@media (prefers-reduced-motion: reduce) {
  .tool-panel__spinner {
    animation-duration: 1.6s;
  }

  .tool-panel__close {
    transition: none;
  }
}
</style>
