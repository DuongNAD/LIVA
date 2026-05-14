<script setup lang="ts">
/**
 * Sidebar.vue — Icon Navigation Sidebar
 * ========================================
 * Vertical icon-based navigation with tooltip labels.
 */
import { ref } from "vue";
import { useI18n } from "../../composables/useI18n";

const emit = defineEmits<{
  (e: 'navigate', page: string): void;
}>();

const { t } = useI18n();
const activePage = ref('avatar');

interface NavItem {
  id: string;
  icon: string;
  labelKey: string;
}

const navItems: NavItem[] = [
  { id: 'avatar',   icon: '', labelKey: 'nav_avatar' },
  { id: 'ai',       icon: '', labelKey: 'nav_settings' },
  { id: 'tasks',    icon: '', labelKey: 'nav_tasks' },
  { id: 'skills',   icon: '', labelKey: 'nav_skills' },
  { id: 'system',   icon: '', labelKey: 'nav_system' },
  { id: 'profile',  icon: '', labelKey: 'nav_profile' },
];

const navigate = (page: string) => {
  activePage.value = page;
  emit('navigate', page);
};
</script>

<template>
  <nav class="sidebar">
    <div class="sidebar-nav">
      <button
        v-for="item in navItems"
        :key="item.id"
        :class="['sidebar-btn', { active: activePage === item.id }]"
        @click="navigate(item.id)"
        :title="t(item.labelKey)"
      >
        <span class="sidebar-icon">
          <svg v-if="item.id === 'avatar'" xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M19 21v-2a4 4 0 0 0-4-4H9a4 4 0 0 0-4 4v2"/><circle cx="12" cy="7" r="4"/></svg>
          <svg v-else-if="item.id === 'ai'" xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><rect width="16" height="16" x="4" y="4" rx="2"/><rect width="6" height="6" x="9" y="9" rx="1"/><path d="M15 2v2"/><path d="M15 20v2"/><path d="M2 15h2"/><path d="M2 9h2"/><path d="M20 15h2"/><path d="M20 9h2"/><path d="M9 2v2"/><path d="M9 20v2"/></svg>
          <svg v-else-if="item.id === 'tasks'" xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><line x1="8" x2="21" y1="6" y2="6"/><line x1="8" x2="21" y1="12" y2="12"/><line x1="8" x2="21" y1="18" y2="18"/><line x1="3" x2="3.01" y1="6" y2="6"/><line x1="3" x2="3.01" y1="12" y2="12"/><line x1="3" x2="3.01" y1="18" y2="18"/></svg>
          <svg v-else-if="item.id === 'skills'" xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><polygon points="13 2 3 14 12 14 11 22 21 10 12 10 13 2"/></svg>
          <svg v-else-if="item.id === 'system'" xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M22 12h-4l-3 9L9 3l-3 9H2"/></svg>
          <svg v-else-if="item.id === 'profile'" xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/><circle cx="12" cy="10" r="3"/><path d="M7 20.662V19a2 2 0 0 1 2-2h6a2 2 0 0 1 2 2v1.662"/></svg>
        </span>
        <span class="sidebar-label">{{ t(item.labelKey) }}</span>
      </button>
    </div>

    <!-- Bottom section -->
    <div class="sidebar-footer">
      <button class="sidebar-btn" :class="{ active: activePage === 'settings' }" @click="navigate('settings')" :title="t('nav_sys_settings')">
        <span class="sidebar-icon">
          <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z"/></svg>
        </span>
        <span class="sidebar-label">{{ t('nav_sys_settings') }}</span>
      </button>
    </div>
  </nav>
</template>

<style scoped>
.sidebar {
  width: var(--sidebar-width, 64px);
  background: var(--sidebar-bg);
  border-right: 1px solid var(--border-default);
  display: flex;
  flex-direction: column;
  justify-content: space-between;
  padding: 8px 0;
  flex-shrink: 0;
}

.sidebar-nav {
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 0 6px;
}

.sidebar-footer {
  padding: 0 6px;
}

.sidebar-btn {
  position: relative;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 2px;
  width: 52px;
  height: 52px;
  border: none;
  background: transparent;
  cursor: pointer;
  border-radius: var(--radius-md);
  transition: all var(--transition-fast);
  color: var(--text-secondary);
}

.sidebar-btn:hover {
  color: var(--text-primary);
}

.sidebar-btn.active {
  color: #818cf8;
}

.sidebar-btn.active .sidebar-icon {
  filter: drop-shadow(0 0 10px rgba(107, 92, 246, 0.8));
}

.sidebar-icon {
  display: flex;
  align-items: center;
  justify-content: center;
}

.sidebar-label {
  font-size: 9px;
  font-weight: 500;
  white-space: nowrap;
  opacity: 0.8;
}
</style>
