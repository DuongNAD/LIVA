import { ref, watch, nextTick, type Ref } from 'vue';
import type { IPlatformAdapter } from '../platform/IPlatformAdapter';
import type { AvatarEngineApi } from './useWidgetAvatarControl';
import type { ToolPanelView } from '../types/ui';
import { logger } from '../utils/logger';

export interface UseWidgetWindowOptions {
  isCollapsed: Ref<boolean>;
  messagesLength: Ref<number>;
  toolPanel: Ref<ToolPanelView | null>;
  chatUIRef: Ref<HTMLElement | null>;
  chatContainer: Ref<HTMLElement | null>;
  miniIconsRef: Ref<HTMLElement | null>;
  toolPanelZoneRef: Ref<HTMLElement | null>;
  engineRef: Ref<AvatarEngineApi | null>;
  platform: IPlatformAdapter | null | undefined;
}

export function useWidgetWindow(options: UseWidgetWindowOptions) {
  const {
    isCollapsed,
    messagesLength,
    toolPanel,
    chatUIRef,
    chatContainer,
    miniIconsRef,
    toolPanelZoneRef,
    engineRef,
    platform,
  } = options;

  // ═══════════════════════════════════════════════════════
  //  Collapse & Snap Logic
  // ═══════════════════════════════════════════════════════
  const snapPosition = ref('right');
  const verticalSnapPosition = ref('bottom');

  // We declare dragOffset here so both Dragging and Collapse can use it.
  const dragOffset = ref({ x: 0, y: 0 });

  const snapToEdge = () => {
    const collapsedWidth = 48; // w-12 is 48px
    const naturalLeft = window.innerWidth - 16 - collapsedWidth;
    const currentCenterX = naturalLeft + dragOffset.value.x + collapsedWidth / 2;

    if (currentCenterX < window.innerWidth / 2) {
      snapPosition.value = 'left';
      dragOffset.value.x = 16 - naturalLeft;
    } else {
      snapPosition.value = 'right';
      dragOffset.value.x = 0;
    }
  };

  const toggleCollapse = () => {
    isCollapsed.value = !isCollapsed.value;
    const currentAbsoluteY = window.innerHeight - 60 + dragOffset.value.y;
    verticalSnapPosition.value = currentAbsoluteY < window.innerHeight / 2 ? 'top' : 'bottom';
  };

  // ═══════════════════════════════════════════════════════
  //  Chat UI Dragging Logic
  // ═══════════════════════════════════════════════════════
  const isDragging = ref(false);
  let startMousePos = { x: 0, y: 0 };
  let startDragOffset = { x: 0, y: 0 };

  const onDragMove = (e: MouseEvent) => {
    if (!isDragging.value) return;
    const nextX = startDragOffset.x + (e.clientX - startMousePos.x);
    const nextY = startDragOffset.y + (e.clientY - startMousePos.y);
    const maxX = Math.max(window.innerWidth - 120, 0);
    const maxY = Math.max(window.innerHeight - 120, 0);
    dragOffset.value = {
      x: Math.min(Math.max(nextX, -maxX), maxX),
      y: Math.min(Math.max(nextY, -maxY), maxY),
    };
  };

  const onDragEnd = () => {
    isDragging.value = false;
    globalThis.document.removeEventListener('mousemove', onDragMove);
    globalThis.document.removeEventListener('mouseup', onDragEnd);

    const currentWidth = isCollapsed.value ? 48 : 400;
    const naturalLeft = window.innerWidth - 16 - currentWidth;
    const currentCenterX = naturalLeft + dragOffset.value.x + currentWidth / 2;
    snapPosition.value = currentCenterX < window.innerWidth / 2 ? 'left' : 'right';

    const currentAbsoluteY = window.innerHeight - 60 + dragOffset.value.y;
    verticalSnapPosition.value = currentAbsoluteY < window.innerHeight / 2 ? 'top' : 'bottom';

    if (isCollapsed.value) {
      snapToEdge();
    }
  };

  const onDragStart = (e: MouseEvent) => {
    isDragging.value = true;
    startMousePos = { x: e.clientX, y: e.clientY };
    startDragOffset = { ...dragOffset.value };
    globalThis.document.addEventListener('mousemove', onDragMove);
    globalThis.document.addEventListener('mouseup', onDragEnd);
  };

  // ═══════════════════════════════════════════════════════
  //  Phantom Bounding Box Fix — Rust Cursor Hit-Test System
  // ═══════════════════════════════════════════════════════
  let zonesInterval: ReturnType<typeof setInterval> | null = null;
  let lastZonesHash = '';
  let currentIntervalMs = 150;
  let idleTickCount = 0;

  const setIntervalRate = (ms: number) => {
    if (currentIntervalMs === ms && zonesInterval) return;
    currentIntervalMs = ms;
    if (zonesInterval) {
      clearInterval(zonesInterval);
      zonesInterval = setInterval(updateInteractiveZones, ms);
    }
  };

  const updateInteractiveZones = () => {
    if (!platform) return;
    const zones: Array<{ x: number; y: number; width: number; height: number }> = [];

    if (chatUIRef.value) {
      const rect = chatUIRef.value.getBoundingClientRect();
      zones.push({
        x: Math.round(rect.left),
        y: Math.round(rect.top),
        width: Math.round(rect.width),
        height: Math.round(rect.height),
      });
    }

    if (!isCollapsed.value && chatContainer.value) {
      const rect = chatContainer.value.getBoundingClientRect();
      zones.push({
        x: Math.round(rect.left),
        y: Math.round(rect.top),
        width: Math.round(rect.width),
        height: Math.round(rect.height),
      });
    }

    if (miniIconsRef.value) {
      const rect = miniIconsRef.value.getBoundingClientRect();
      zones.push({
        x: Math.round(rect.left),
        y: Math.round(rect.top),
        width: Math.round(rect.width),
        height: Math.round(rect.height),
      });
    }

    if (toolPanel.value && toolPanelZoneRef.value) {
      const rect = toolPanelZoneRef.value.getBoundingClientRect();
      if (rect.width > 0 && rect.height > 0) {
        zones.push({
          x: Math.round(rect.left),
          y: Math.round(rect.top),
          width: Math.round(rect.width),
          height: Math.round(rect.height),
        });
      }
    }

    const avatarBounds = engineRef.value?.getScreenBounds?.();
    const isDraggingAvatar = (engineRef.value as { isDragging?: { value?: boolean } } | null)?.isDragging?.value;

    if (isDraggingAvatar) {
      zones.push({
        x: 0,
        y: 0,
        width: window.innerWidth,
        height: window.innerHeight,
      });
    } else if (avatarBounds && avatarBounds.width > 0 && avatarBounds.height > 0) {
      const pad = 12;
      zones.push({
        x: Math.round(Math.max(avatarBounds.x - pad, 0)),
        y: Math.round(Math.max(avatarBounds.y - pad, 0)),
        width: Math.round(avatarBounds.width + pad * 2),
        height: Math.round(avatarBounds.height + pad * 2),
      });
    }

    const currentHash = JSON.stringify(zones);
    if (currentHash === lastZonesHash) {
      idleTickCount++;
      if (idleTickCount >= 3 && currentIntervalMs === 150) {
        // Window is stationary and idle: throttle check from 150ms to 500ms
        setIntervalRate(500);
      }
      return;
    }

    lastZonesHash = currentHash;
    idleTickCount = 0;
    if (currentIntervalMs !== 150) {
      setIntervalRate(150);
    }

    platform.invokeBackend('update_interactive_zones', { zones }).catch((err) => {
      logger.warn('[Widget] Failed to update interactive zones:', err);
    });
  };

  const startZonesInterval = () => {
    if (!zonesInterval) {
      currentIntervalMs = 150;
      idleTickCount = 0;
      zonesInterval = setInterval(updateInteractiveZones, currentIntervalMs);
    }
  };

  const pauseZonesInterval = () => {
    if (zonesInterval) {
      clearInterval(zonesInterval);
      zonesInterval = null;
    }
  };

  watch(
    [isCollapsed, isDragging, messagesLength, toolPanel],
    () => {
      idleTickCount = 0;
      if (currentIntervalMs !== 150) {
        setIntervalRate(150);
      }
      nextTick(() => {
        updateInteractiveZones();
      });
    },
    { deep: true }
  );

  return {
    dragOffset,
    isDragging,
    onDragStart,
    onDragMove,
    onDragEnd,
    snapPosition,
    verticalSnapPosition,
    snapToEdge,
    toggleCollapse,
    updateInteractiveZones,
    startZonesInterval,
    pauseZonesInterval,
  };
}
