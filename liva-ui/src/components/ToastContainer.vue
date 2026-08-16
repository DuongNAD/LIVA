<script setup lang="ts">
/**
 * ToastContainer.vue — Global Toast Presentation Layer
 * ====================================================
 * Renders floating toast notifications with slide-in animations,
 * type-based color accents, auto-dismiss, and action handlers.
 */
import { useToast } from '../composables/useToast';
import type { ToastItem } from '../composables/useToast';

const { toasts, dismiss } = useToast();

const handleAction = (toast: ToastItem) => {
  if (toast.action?.onClick) {
    toast.action.onClick();
  }
  dismiss(toast.id);
};
</script>

<template>
  <div class="toast-container" aria-live="polite" aria-atomic="false">
    <TransitionGroup name="toast-slide" tag="div" class="toast-list">
      <div
        v-for="toast in toasts"
        :key="toast.id"
        :class="['toast-item', `toast-${toast.type}`]"
        :role="toast.type === 'error' ? 'alert' : 'status'"
      >
        <!-- Icon -->
        <div class="toast-icon">
          <svg v-if="toast.type === 'success'" xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <polyline points="20 6 9 17 4 12"></polyline>
          </svg>
          <svg v-else-if="toast.type === 'error'" xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <circle cx="12" cy="12" r="10"></circle>
            <line x1="15" y1="9" x2="9" y2="15"></line>
            <line x1="9" y1="9" x2="15" y2="15"></line>
          </svg>
          <svg v-else-if="toast.type === 'warning'" xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <path d="m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3Z"></path>
            <line x1="12" y1="9" x2="12" y2="13"></line>
            <line x1="12" y1="17" x2="12.01" y2="17"></line>
          </svg>
          <svg v-else xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <circle cx="12" cy="12" r="10"></circle>
            <line x1="12" y1="16" x2="12" y2="12"></line>
            <line x1="12" y1="8" x2="12.01" y2="8"></line>
          </svg>
        </div>

        <!-- Content -->
        <div class="toast-body">
          <div v-if="toast.title" class="toast-title">{{ toast.title }}</div>
          <div class="toast-message">{{ toast.message }}</div>
        </div>

        <!-- Action Button -->
        <button
          v-if="toast.action"
          class="toast-action-btn"
          @click="handleAction(toast)"
        >
          {{ toast.action.label }}
        </button>

        <!-- Dismiss Button -->
        <button
          v-if="toast.dismissible"
          class="toast-close-btn"
          aria-label="Close notification"
          @click="dismiss(toast.id)"
        >
          &times;
        </button>
      </div>
    </TransitionGroup>
  </div>
</template>

<style scoped>
.toast-container {
  position: fixed;
  top: 48px;
  right: 20px;
  z-index: 10000;
  pointer-events: none;
  max-width: 420px;
  width: calc(100vw - 40px);
}

.toast-list {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.toast-item {
  pointer-events: auto;
  display: flex;
  align-items: flex-start;
  gap: 12px;
  padding: 12px 14px;
  border-radius: var(--radius-md, 12px);
  background: rgba(18, 22, 34, 0.92);
  backdrop-filter: blur(12px);
  -webkit-backdrop-filter: blur(12px);
  border: 1px solid var(--border-default, rgba(255, 255, 255, 0.08));
  box-shadow: 0 10px 25px -5px rgba(0, 0, 0, 0.5), 0 8px 10px -6px rgba(0, 0, 0, 0.3);
  color: var(--text-primary, #f1f5f9);
  transition: all 0.25s cubic-bezier(0.4, 0, 0.2, 1);
}

/* Types */
.toast-success {
  border-left: 4px solid var(--color-success, #10b981);
}
.toast-success .toast-icon {
  color: var(--color-success, #10b981);
}

.toast-error {
  border-left: 4px solid var(--color-danger, #ef4444);
}
.toast-error .toast-icon {
  color: var(--color-danger, #ef4444);
}

.toast-warning {
  border-left: 4px solid var(--color-warning, #f59e0b);
}
.toast-warning .toast-icon {
  color: var(--color-warning, #f59e0b);
}

.toast-info {
  border-left: 4px solid var(--color-info, #0ea5e9);
}
.toast-info .toast-icon {
  color: var(--color-info, #0ea5e9);
}

.toast-icon {
  flex-shrink: 0;
  display: flex;
  align-items: center;
  margin-top: 1px;
}

.toast-body {
  flex: 1;
  min-width: 0;
}

.toast-title {
  font-size: 13px;
  font-weight: 600;
  margin-bottom: 2px;
  line-height: 1.3;
}

.toast-message {
  font-size: 12px;
  color: var(--text-secondary, #94a3b8);
  line-height: 1.4;
  word-break: break-word;
}

.toast-action-btn {
  flex-shrink: 0;
  padding: 4px 10px;
  border-radius: var(--radius-sm, 6px);
  background: rgba(255, 255, 255, 0.08);
  border: 1px solid rgba(255, 255, 255, 0.12);
  color: var(--text-primary, #f1f5f9);
  font-size: 11px;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.15s ease;
  align-self: center;
}

.toast-action-btn:hover {
  background: rgba(255, 255, 255, 0.16);
  border-color: rgba(255, 255, 255, 0.2);
}

.toast-close-btn {
  flex-shrink: 0;
  background: transparent;
  border: none;
  color: var(--text-muted, #64748b);
  font-size: 16px;
  line-height: 1;
  cursor: pointer;
  padding: 2px;
  border-radius: 4px;
  transition: color 0.15s ease;
  align-self: flex-start;
}

.toast-close-btn:hover {
  color: var(--text-primary, #f1f5f9);
}

/* Animations */
.toast-slide-enter-active {
  animation: toastIn 0.3s cubic-bezier(0.16, 1, 0.3, 1);
}

.toast-slide-leave-active {
  animation: toastOut 0.2s cubic-bezier(0.4, 0, 1, 1);
  position: absolute;
  width: 100%;
}

.toast-slide-move {
  transition: transform 0.25s ease;
}

@keyframes toastIn {
  from {
    opacity: 0;
    transform: translateX(40px) scale(0.95);
  }
  to {
    opacity: 1;
    transform: translateX(0) scale(1);
  }
}

@keyframes toastOut {
  from {
    opacity: 1;
    transform: translateX(0) scale(1);
  }
  to {
    opacity: 0;
    transform: translateX(40px) scale(0.9);
  }
}
</style>
