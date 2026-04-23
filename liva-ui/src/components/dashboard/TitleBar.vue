<script setup lang="ts">
/**
 * TitleBar.vue — Custom Window Titlebar
 * =======================================
 * Frameless window titlebar with drag area, LIVA branding, and window controls.
 */

const electronAPI = (window as any).electronAPI;

const minimize = () => electronAPI?.minimizeWindow();
const maximize = () => electronAPI?.maximizeWindow();
const close = () => electronAPI?.closeDashboard();
</script>

<template>
  <header class="titlebar" style="-webkit-app-region: drag;">
    <!-- Branding -->
    <div class="titlebar-brand">
      <div class="titlebar-logo">
        <span class="logo-icon">🤖</span>
        <span class="logo-text">LIVA</span>
        <span class="logo-badge">Dashboard</span>
      </div>
    </div>

    <!-- Window Controls -->
    <div class="titlebar-controls" style="-webkit-app-region: no-drag;">
      <button class="titlebar-btn" @click="minimize" title="Thu nhỏ">
        <svg width="12" height="12" viewBox="0 0 12 12"><rect y="5" width="12" height="2" fill="currentColor" rx="1"/></svg>
      </button>
      <button class="titlebar-btn" @click="maximize" title="Phóng to">
        <svg width="12" height="12" viewBox="0 0 12 12"><rect x="1" y="1" width="10" height="10" stroke="currentColor" stroke-width="1.5" fill="none" rx="1.5"/></svg>
      </button>
      <button class="titlebar-btn titlebar-btn-close" @click="close" title="Đóng">
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
