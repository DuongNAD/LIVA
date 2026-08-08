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

  const updateInteractiveZones = () => {
    if (!platform) return;
    const zones: Array<{ x: number; y: number; width: number; height: number }> = [];

    if (chatUIRef.value) {
      const rect = chatUIRef.value.getBoundingClientRect();
      zones.push({
        x: rect.left,
        y: rect.top,
        width: rect.width,
        height: rect.height,
      });
    }

    if (!isCollapsed.value && chatContainer.value) {
      const rect = chatContainer.value.getBoundingClientRect();
      zones.push({
        x: rect.left,
        y: rect.top,
        width: rect.width,
        height: rect.height,
      });
    }

    if (miniIconsRef.value) {
      const rect = miniIconsRef.value.getBoundingClientRect();
      zones.push({
        x: rect.left,
        y: rect.top,
        width: rect.width,
        height: rect.height,
      });
    }

    if (toolPanel.value && toolPanelZoneRef.value) {
      const rect = toolPanelZoneRef.value.getBoundingClientRect();
      if (rect.width > 0 && rect.height > 0) {
        zones.push({
          x: rect.left,
          y: rect.top,
          width: rect.width,
          height: rect.height,
        });
      }
    }

    const avatarBounds = engineRef.value?.getScreenBounds?.();
    if (avatarBounds && avatarBounds.width > 0 && avatarBounds.height > 0) {
      zones.push(avatarBounds);
    }

    platform.invokeBackend('update_interactive_zones', { zones }).catch((err) => {
      logger.warn('[Widget] Failed to update interactive zones:', err);
    });
  };

  const startZonesInterval = () => {
    if (!zonesInterval) {
      zonesInterval = setInterval(updateInteractiveZones, 150);
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
