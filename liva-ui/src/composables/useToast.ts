/**
 * useToast.ts — Global Toast Notification Composable
 * ===================================================
 * Centralized reactive notification queue for LIVA UI.
 * Provides typed toast notifications (info, success, warning, error)
 * with auto-dismiss timers, custom actions, and singleton state.
 */

import { ref, readonly } from 'vue';

export type ToastType = 'info' | 'success' | 'warning' | 'error';

export interface ToastAction {
  label: string;
  onClick: () => void;
}

export interface ToastOptions {
  id?: string;
  title?: string;
  message: string;
  type?: ToastType;
  duration?: number; // ms, 0 means persistent (no auto-dismiss)
  dismissible?: boolean;
  action?: ToastAction;
}

export interface ToastItem {
  id: string;
  title?: string;
  message: string;
  type: ToastType;
  duration: number;
  dismissible: boolean;
  action?: ToastAction;
  createdAt: number;
  timer?: ReturnType<typeof setTimeout>;
}

// Global shared reactive state
const toasts = ref<ToastItem[]>([]);
let counter = 0;

const DEFAULT_DURATIONS: Record<ToastType, number> = {
  info: 4000,
  success: 3500,
  warning: 5000,
  error: 6500,
};

export function useToast() {
  /**
   * Dismiss a toast by id
   */
  const dismiss = (id: string) => {
    const index = toasts.value.findIndex((t) => t.id === id);
    if (index !== -1) {
      const toast = toasts.value[index];
      if (toast.timer) {
        clearTimeout(toast.timer);
      }
      toasts.value.splice(index, 1);
    }
  };

  /**
   * Clear all active toasts
   */
  const clear = () => {
    for (const t of toasts.value) {
      if (t.timer) {
        clearTimeout(t.timer);
      }
    }
    toasts.value = [];
  };

  /**
   * Show a new toast notification
   */
  const show = (options: ToastOptions | string): string => {
    const opts: ToastOptions =
      typeof options === 'string' ? { message: options } : options;

    const id = opts.id || `toast-${Date.now()}-${++counter}`;
    const type: ToastType = opts.type || 'info';
    const duration =
      opts.duration !== undefined ? opts.duration : DEFAULT_DURATIONS[type];
    const dismissible = opts.dismissible !== undefined ? opts.dismissible : true;

    // If a toast with the same id exists, remove it first
    dismiss(id);

    const toastItem: ToastItem = {
      id,
      title: opts.title,
      message: opts.message,
      type,
      duration,
      dismissible,
      action: opts.action,
      createdAt: Date.now(),
    };

    if (duration > 0) {
      toastItem.timer = setTimeout(() => {
        dismiss(id);
      }, duration);
    }

    // Limit maximum active toasts to 6 to prevent viewport clutter
    if (toasts.value.length >= 6) {
      const oldest = toasts.value[0];
      if (oldest) {
        dismiss(oldest.id);
      }
    }

    toasts.value.push(toastItem);
    return id;
  };

  /**
   * Convenience helpers
   */
  const success = (
    message: string,
    options?: Omit<ToastOptions, 'message' | 'type'>
  ) => show({ ...options, message, type: 'success' });

  const error = (
    message: string,
    options?: Omit<ToastOptions, 'message' | 'type'>
  ) => show({ ...options, message, type: 'error' });

  const warning = (
    message: string,
    options?: Omit<ToastOptions, 'message' | 'type'>
  ) => show({ ...options, message, type: 'warning' });

  const warn = warning;

  const info = (
    message: string,
    options?: Omit<ToastOptions, 'message' | 'type'>
  ) => show({ ...options, message, type: 'info' });

  return {
    toasts: readonly(toasts),
    show,
    success,
    error,
    warning,
    warn,
    info,
    dismiss,
    clear,
  };
}
