<script setup lang="ts">
/**
 * Sidebar.vue — Icon Navigation Sidebar
 * ========================================
 * Vertical icon-based navigation with tooltip labels.
 */
import { ref } from "vue";

const emit = defineEmits<{
  (e: 'navigate', page: string): void;
}>();

const activePage = ref('avatar');

interface NavItem {
  id: string;
  icon: string;
  label: string;
}

const navItems: NavItem[] = [
  { id: 'avatar',   icon: '🎭', label: 'Avatar' },
  { id: 'ai',       icon: '🤖', label: 'AI Settings' },
  { id: 'tasks',    icon: '📝', label: 'Tasks' },
  { id: 'skills',   icon: '⚡', label: 'Skills' },
  { id: 'system',   icon: '📊', label: 'System' },
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
        :title="item.label"
      >
        <span class="sidebar-icon">{{ item.icon }}</span>
        <span class="sidebar-label">{{ item.label }}</span>
        <!-- Active indicator -->
        <div v-if="activePage === item.id" class="sidebar-indicator"></div>
      </button>
    </div>

    <!-- Bottom section -->
    <div class="sidebar-footer">
      <button class="sidebar-btn" @click="navigate('settings')" title="Settings">
        <span class="sidebar-icon">⚙️</span>
        <span class="sidebar-label">Cài đặt</span>
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
  background: var(--bg-hover);
  color: var(--text-primary);
}

.sidebar-btn.active {
  background: rgba(124, 58, 237, 0.12);
  color: var(--text-primary);
}

.sidebar-icon {
  font-size: 20px;
  line-height: 1;
}

.sidebar-label {
  font-size: 9px;
  font-weight: 500;
  white-space: nowrap;
  opacity: 0.8;
}

.sidebar-indicator {
  position: absolute;
  left: -6px;
  top: 50%;
  transform: translateY(-50%);
  width: 3px;
  height: 20px;
  background: var(--accent-gradient);
  border-radius: 0 3px 3px 0;
}
</style>
