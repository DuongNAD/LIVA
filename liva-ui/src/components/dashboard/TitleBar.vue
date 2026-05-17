<script setup lang="ts">
/**
 * TitleBar.vue — Custom Window Titlebar
 * =======================================
 * Frameless window titlebar with drag area, LIVA branding, and window controls.
 */

import { ref, onMounted, watch } from 'vue';
import { useI18n } from '../../composables/useI18n';

const { t } = useI18n();

const minimize = async () => {
  try {
    const { getCurrentWindow } = await import('@tauri-apps/api/window');
    await getCurrentWindow().minimize();
  } catch { /* Browser dev mode — no-op */ }
};

const maximize = async () => {
  try {
    const { getCurrentWindow } = await import('@tauri-apps/api/window');
    const win = getCurrentWindow();
    if (await win.isMaximized()) {
      await win.unmaximize();
    } else {
      await win.maximize();
    }
  } catch { /* Browser dev mode — no-op */ }
};

const close = async () => {
  try {
    const { getCurrentWindow } = await import('@tauri-apps/api/window');
    await getCurrentWindow().hide();
  } catch { /* Browser dev mode — no-op */ }
};

// Theme State
const isLightMode = ref(localStorage.getItem('dashboard_theme') === 'light');

onMounted(() => {
  if (isLightMode.value) {
    document.documentElement.setAttribute('data-theme', 'light');
    document.body.setAttribute('data-theme', 'light');
    document.getElementById('app')?.setAttribute('data-theme', 'light');
  }
});

watch(isLightMode, (val) => {
  localStorage.setItem('dashboard_theme', val ? 'light' : 'dark');
  if (val) {
    document.documentElement.setAttribute('data-theme', 'light');
    document.body.setAttribute('data-theme', 'light');
    document.getElementById('app')?.setAttribute('data-theme', 'light');
  } else {
    document.documentElement.removeAttribute('data-theme');
    document.body.removeAttribute('data-theme');
    document.getElementById('app')?.removeAttribute('data-theme');
  }
});
</script>

<template>
  <header class="titlebar" style="-webkit-app-region: drag;">
    <!-- Branding -->
    <div class="titlebar-brand">
      <div class="titlebar-logo">
        <img src="/liva-logo.png" alt="LIVA" class="w-5 h-5 object-contain drop-shadow-[0_0_8px_rgba(107,92,246,0.5)]" />
        <span class="logo-text">LIVA</span>
        <span class="logo-badge">Dashboard</span>
      </div>
    </div>

    <!-- Window Controls -->
    <div class="titlebar-controls" style="-webkit-app-region: no-drag;">
      <!-- Theme Toggle -->
      <button class="titlebar-btn" @click="isLightMode = !isLightMode" :title="isLightMode ? t('tb_theme_dark') : t('tb_theme_light')">
        <!-- Moon Icon -->
        <svg v-if="isLightMode" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.5" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" d="M21.752 15.002A9.718 9.718 0 0118 15.75c-5.385 0-9.75-4.365-9.75-9.75 0-1.33.266-2.597.748-3.752A9.753 9.753 0 003 11.25C3 16.635 7.365 21 12.75 21a9.753 9.753 0 009.002-5.998z" />
        </svg>
        <!-- Sun Icon -->
        <svg v-else width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.5" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" d="M12 3v2.25m6.364.386l-1.591 1.591M21 12h-2.25m-.386 6.364l-1.591-1.591M12 18.75V21m-4.773-4.227l-1.591 1.591M5.25 12H3m4.227-4.773L5.636 5.636M15.75 12a3.75 3.75 0 11-7.5 0 3.75 3.75 0 017.5 0z" />
        </svg>
      </button>

      <button class="titlebar-btn" @click="minimize" :title="t('tb_min')">
        <svg width="12" height="12" viewBox="0 0 12 12"><rect y="5" width="12" height="2" fill="currentColor" rx="1"/></svg>
      </button>
      <button class="titlebar-btn" @click="maximize" :title="t('tb_max')">
        <svg width="12" height="12" viewBox="0 0 12 12"><rect x="1" y="1" width="10" height="10" stroke="currentColor" stroke-width="1.5" fill="none" rx="1.5"/></svg>
      </button>
      <button class="titlebar-btn titlebar-btn-close" @click="close" :title="t('tb_close')">
        <svg width="12" height="12" viewBox="0 0 12 12"><path d="M2 2l8 8M10 2l-8 8" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/></svg>
      </button>
    </div>
  </header>
</template>

<style scoped>
.titlebar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  height: var(--titlebar-height, 38px);
  background: var(--bg-primary);
  border-bottom: 1px solid var(--border-default);
  padding: 0 8px 0 16px;
  flex-shrink: 0;
}

.titlebar-brand {
  display: flex;
  align-items: center;
}

.titlebar-logo {
  display: flex;
  align-items: center;
  gap: 8px;
}

.logo-icon {
  font-size: 16px;
}

.logo-text {
  font-size: 13px;
  font-weight: 700;
  background: var(--accent-gradient);
  -webkit-background-clip: text;
  -webkit-text-fill-color: transparent;
  background-clip: text;
  letter-spacing: 1px;
}

.logo-badge {
  font-size: 10px;
  color: var(--text-muted);
  font-weight: 500;
  padding: 1px 6px;
  border: 1px solid var(--border-default);
  border-radius: 4px;
}

.titlebar-controls {
  display: flex;
  align-items: center;
  gap: 2px;
}

.titlebar-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 36px;
  height: 28px;
  border: none;
  background: transparent;
  color: var(--text-secondary);
  cursor: pointer;
  border-radius: 4px;
  transition: all 0.15s ease;
}

.titlebar-btn:hover {
  background: var(--bg-hover);
  color: var(--text-primary);
}

.titlebar-btn-close:hover {
  background: var(--color-danger);
  color: white;
}
</style>
