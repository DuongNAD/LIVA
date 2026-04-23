<script setup lang="ts">
/**
 * DashboardApp.vue — Root Layout for Dashboard Window
 * =====================================================
 * Custom titlebar + Sidebar + Dynamic content area + Status bar.
 * Single-page app with component switching via sidebar navigation.
 */
import { ref, shallowRef, markRaw, onMounted, onUnmounted, computed } from "vue";
import { useGateway } from "./composables/useGateway";

import TitleBar from "./components/dashboard/TitleBar.vue";
import Sidebar from "./components/dashboard/Sidebar.vue";
import StatusBar from "./components/dashboard/StatusBar.vue";

import AvatarGallery from "./components/dashboard/AvatarGallery.vue";
import AISettings from "./components/dashboard/AISettings.vue";
import TaskManager from "./components/dashboard/TaskManager.vue";
import SkillsView from "./components/dashboard/SkillsView.vue";
import SystemView from "./components/dashboard/SystemView.vue";

// Page mapping
const pageMap: Record<string, any> = {
  avatar: markRaw(AvatarGallery),
  ai: markRaw(AISettings),
  tasks: markRaw(TaskManager),
  skills: markRaw(SkillsView),
  system: markRaw(SystemView),
  settings: markRaw(AISettings),
};

const activePageId = ref('avatar');
const activePage = shallowRef<any>(pageMap['avatar']);

const onNavigate = (page: string) => {
  activePageId.value = page;
  activePage.value = pageMap[page] || pageMap['avatar'];
};

const gateway = useGateway();
const gpuSetupStatus = computed(() => gateway.gpuSetupStatus.value);

onMounted(() => {
  gateway.init();
});

onUnmounted(() => {
  gateway.destroy();
});
</script>

<template>
  <div class="dashboard-layout">
    <!-- Custom Titlebar -->
    <TitleBar />

    <!-- Main Content -->
    <div class="dashboard-body">
      <!-- Sidebar -->
      <Sidebar @navigate="onNavigate" />

      <!-- Content Area -->
      <main class="dashboard-content">
        <KeepAlive>
          <component :is="activePage" :key="activePageId" />
        </KeepAlive>
      </main>
    </div>

    <!-- Status Bar -->
    <StatusBar />

    <!-- GPU Setup Splash Screen (Overlay) -->
    <div v-if="gpuSetupStatus" class="gpu-setup-overlay animate-fadeIn">
      <div class="gpu-setup-card">
        <div class="gpu-icon-pulse">🎮</div>
        <h2 class="gpu-setup-title">Smart GPU Wizard</h2>
        <p class="gpu-setup-text">{{ gpuSetupStatus }}</p>
        <div class="gpu-setup-loader"></div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.dashboard-layout {
  display: flex;
  flex-direction: column;
  height: 100vh;
  width: 100vw;
  background: var(--bg-primary);
  overflow: hidden;
}

.dashboard-body {
  display: flex;
  flex: 1;
  overflow: hidden;
}

.dashboard-content {
  flex: 1;
  overflow: hidden;
  background: var(--bg-secondary);
  border-radius: var(--radius-md) 0 0 0;
}

/* Loading State */
.loading-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  height: 100%;
  gap: var(--space-md);
  color: var(--text-muted);
  font-size: 14px;
}

.loading-spinner {
  width: 32px;
  height: 32px;
  border: 3px solid var(--border-default);
  border-top-color: var(--accent-start);
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
}

@keyframes spin {
  to { transform: rotate(360deg); }
}

/* GPU Setup Overlay */
.gpu-setup-overlay {
  position: absolute;
  top: 0; left: 0; right: 0; bottom: 0;
  background: rgba(10, 10, 12, 0.95);
  backdrop-filter: blur(8px);
  z-index: 9999;
  display: flex;
  align-items: center;
  justify-content: center;
}

.gpu-setup-card {
  background: var(--bg-secondary);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-lg);
  padding: 40px;
  text-align: center;
  box-shadow: 0 20px 40px rgba(0,0,0,0.5);
  max-width: 400px;
  width: 100%;
}

.gpu-icon-pulse {
  font-size: 48px;
  margin-bottom: 20px;
  animation: pulse 1.5s infinite;
}

.gpu-setup-title {
  color: var(--text-primary);
  font-size: 18px;
  font-weight: 700;
  margin-bottom: 12px;
  background: linear-gradient(135deg, var(--accent-start), var(--accent-end));
  -webkit-background-clip: text;
  -webkit-text-fill-color: transparent;
}

.gpu-setup-text {
  color: var(--text-secondary);
  font-size: 14px;
  line-height: 1.5;
  margin-bottom: 24px;
}

.gpu-setup-loader {
  width: 100%;
  height: 4px;
  background: var(--border-default);
  border-radius: 2px;
  overflow: hidden;
  position: relative;
}

.gpu-setup-loader::after {
  content: '';
  position: absolute;
  top: 0; left: 0; bottom: 0;
  width: 40%;
  background: linear-gradient(90deg, var(--accent-start), var(--accent-end));
  border-radius: 2px;
  animation: loadSweep 1.5s infinite ease-in-out;
}

@keyframes loadSweep {
  0% { transform: translateX(-100%); }
  100% { transform: translateX(250%); }
}
</style>
