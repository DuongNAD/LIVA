import { ref, computed } from 'vue';
import type { DiffReviewSession, HunkStatus } from '../types/diff';
import { getPlatformAdapter } from '../platform';
import { logger } from '../utils/logger';

export interface CanvasWidget {
  widget_id: string;
  title: string;
  component_type: string;
  html: string;
  css?: string;
  js?: string;
  props: Record<string, unknown>;
  version: number;
  interactive: boolean;
}

export type LayoutMode = 'diff' | 'canvas' | 'hybrid';
export type StreamStatus = 'idle' | 'streaming' | 'ready' | 'error';

// Singleton State across Vue components
const layoutMode = ref<LayoutMode>('hybrid');
const splitRatio = ref<number>(0.5);
const activeSession = ref<DiffReviewSession | null>(null);
const activeWidget = ref<CanvasWidget | null>(null);
const widgetStreamStatus = ref<StreamStatus>('idle');
const streamProgress = ref<{ receivedChunks: number; totalBytes: number }>({
  receivedChunks: 0,
  totalBytes: 0,
});
const widgetError = ref<string | null>(null);
const isSubmitting = ref<boolean>(false);

export function useLivingCanvas() {
  const adapter = getPlatformAdapter();

  const pendingHunksCount = computed(() => {
    if (!activeSession.value || !Array.isArray(activeSession.value.files)) return 0;
    let count = 0;
    for (const file of activeSession.value.files) {
      if (!Array.isArray(file.hunks)) continue;
      for (const hunk of file.hunks) {
        if (hunk.status && hunk.status.type === 'pending') {
          count++;
        }
      }
    }
    return count;
  });

  const isAllDecided = computed(() => {
    return pendingHunksCount.value === 0 && activeSession.value !== null && Array.isArray(activeSession.value.files);
  });

  const setLayoutMode = (mode: LayoutMode) => {
    layoutMode.value = mode;
  };

  const setSplitRatio = (ratio: number) => {
    splitRatio.value = Math.max(0.1, Math.min(0.9, ratio));
  };

  const fetchPendingHunks = async (sessionId?: string) => {
    try {
      const res = (await adapter.invokeBackend('diff:get_pending_hunks', {
        session_id: sessionId,
      })) as DiffReviewSession | DiffReviewSession[] | null;

      if (Array.isArray(res)) {
        activeSession.value = res.length > 0 && res[0].files ? res[0] : null;
      } else if (res && typeof res === 'object' && Array.isArray(res.files)) {
        activeSession.value = res;
      } else {
        activeSession.value = null;
      }
    } catch (e: unknown) {
      logger.warn('[useLivingCanvas]', 'Failed to fetch pending hunks:', e);
    }
  };

  const submitHunkDecision = async (
    sessionId: string,
    hunkId: string,
    status: HunkStatus
  ) => {
    if (!activeSession.value || activeSession.value.session_id !== sessionId) return;

    // Optimistic Update
    const previousState: HunkStatus | undefined = findHunkStatus(activeSession.value, hunkId);
    applyHunkStatusLocally(activeSession.value, hunkId, status);

    try {
      const payload: Record<string, unknown> = {
        session_id: sessionId,
        hunk_id: hunkId,
        decision: status.type,
      };

      if (status.type === 'rejected' && status.payload?.reason) {
        payload.reason = status.payload.reason;
      } else if (status.type === 'modified' && status.payload?.user_override) {
        payload.custom_content = status.payload.user_override;
      }

      const res = (await adapter.invokeBackend(
        'agent:submit_hunk_decision',
        payload
      )) as { session?: DiffReviewSession };

      if (res && res.session) {
        activeSession.value = res.session;
      }
    } catch (e) {
      logger.error('[useLivingCanvas]', 'Error submitting hunk decision, rolling back:', e);
      if (previousState) {
        applyHunkStatusLocally(activeSession.value, hunkId, previousState);
      }
    }
  };

  const approveAllPending = async () => {
    if (!activeSession.value) return;
    isSubmitting.value = true;
    try {
      const res = (await adapter.invokeBackend('agent:submit_hunk_decision', {
        session_id: activeSession.value.session_id,
        batch: 'approve_all',
      })) as { session?: DiffReviewSession };

      if (res && res.session) {
        activeSession.value = res.session;
      }
    } catch (e) {
      logger.error('[useLivingCanvas]', 'Error approving all hunks:', e);
    } finally {
      isSubmitting.value = false;
    }
  };

  const rejectAllPending = async () => {
    if (!activeSession.value) return;
    isSubmitting.value = true;
    try {
      const res = (await adapter.invokeBackend('agent:submit_hunk_decision', {
        session_id: activeSession.value.session_id,
        batch: 'reject_all',
      })) as { session?: DiffReviewSession };

      if (res && res.session) {
        activeSession.value = res.session;
      }
    } catch (e) {
      logger.error('[useLivingCanvas]', 'Error rejecting all hunks:', e);
    } finally {
      isSubmitting.value = false;
    }
  };

  const submitSessionDecisions = async () => {
    if (!activeSession.value) return;
    isSubmitting.value = true;
    try {
      // Re-fetch or confirm final state with backend
      await fetchPendingHunks(activeSession.value.session_id);
    } catch (e) {
      logger.error('[useLivingCanvas]', 'Error finalizing session:', e);
    } finally {
      isSubmitting.value = false;
    }
  };

  const fetchCanvasState = async () => {
    try {
      const res = (await adapter.invokeBackend('canvas:get_canvas_state', {})) as {
        active_widgets?: Record<string, CanvasWidget>;
        layout?: { split_ratio: number; active_mode: LayoutMode };
      };

      if (res && res.active_widgets) {
        const widgets = Object.values(res.active_widgets);
        if (widgets.length > 0) {
          activeWidget.value = widgets[0];
          widgetStreamStatus.value = 'ready';
        }
      }
      if (res && res.layout) {
        if (res.layout.active_mode) layoutMode.value = res.layout.active_mode;
        if (res.layout.split_ratio) splitRatio.value = res.layout.split_ratio;
      }
    } catch (e) {
      logger.warn('[useLivingCanvas]', 'Failed to fetch canvas state:', e);
    }
  };

  const sendWidgetAction = async (
    widgetId: string,
    action: string,
    payload: unknown
  ) => {
    try {
      await adapter.invokeBackend('canvas:update_widget_state', {
        widget_id: widgetId,
        props: { action, payload },
      });
    } catch (e) {
      logger.error('[useLivingCanvas]', 'Failed to dispatch widget action:', e);
    }
  };

  const setMockSession = (session: DiffReviewSession) => {
    activeSession.value = session;
  };

  const setMockWidget = (widget: CanvasWidget) => {
    activeWidget.value = widget;
    widgetStreamStatus.value = 'ready';
  };

  return {
    layoutMode,
    splitRatio,
    activeSession,
    activeWidget,
    widgetStreamStatus,
    streamProgress,
    widgetError,
    isSubmitting,
    pendingHunksCount,
    isAllDecided,
    setLayoutMode,
    setSplitRatio,
    fetchPendingHunks,
    submitHunkDecision,
    approveAllPending,
    rejectAllPending,
    submitSessionDecisions,
    fetchCanvasState,
    sendWidgetAction,
    setMockSession,
    setMockWidget,
  };
}

function findHunkStatus(session: DiffReviewSession, hunkId: string): HunkStatus | undefined {
  if (!session || !Array.isArray(session.files)) return undefined;
  for (const file of session.files) {
    if (!Array.isArray(file.hunks)) continue;
    for (const hunk of file.hunks) {
      if (hunk.hunk_id === hunkId) {
        return hunk.status;
      }
    }
  }
  return undefined;
}

function applyHunkStatusLocally(
  session: DiffReviewSession,
  hunkId: string,
  status: HunkStatus
) {
  if (!session || !Array.isArray(session.files)) return;
  for (const file of session.files) {
    if (!Array.isArray(file.hunks)) continue;
    for (const hunk of file.hunks) {
      if (hunk.hunk_id === hunkId) {
        hunk.status = status;
        return;
      }
    }
  }
}
