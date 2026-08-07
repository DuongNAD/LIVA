import { ref, watch, onUnmounted, onDeactivated, type Ref } from 'vue';
import { AvatarControlTagStream, type AvatarAction } from '../utils/avatarControlTags';
import { getAvatarAnimation } from '../utils/avatarAnimationRegistry';

/** Các API mà engine avatar (VRM/Live2D) expose qua defineExpose */
export interface AvatarEngineApi {
  triggerMotion?: () => void;
  startAudioLipSync?: (analyser: AnalyserNode) => void;
  stopAudioLipSync?: () => void;
  setExpression?: (emotion: string) => void;
  captureFrameForAI?: () => string | null;
  isCameraOn?: { value: boolean };
  setScreenPosition?: (nx: number, ny: number) => void;
  setScale?: (scale: number) => void;
  getScreenBounds?: () => { x: number; y: number; width: number; height: number } | null;
  moveTo?: (x: number, y: number, options?: { run?: boolean }) => void;
  jump?: () => void;
  stopMoving?: () => void;
  setWander?: (enabled: boolean) => void;
  playGesture?: (name: 'wave' | 'nod' | 'shake') => void;
  setThinking?: (active: boolean) => void;
  inspectScreenPoint?: (x: number, y: number) => void;
  clearInspection?: () => void;
}

export interface UseWidgetAvatarControlOptions {
  isThinking: Ref<boolean>;
  toolPresentationActive: Ref<boolean>;
  engineRef: Ref<AvatarEngineApi | null>;
}

/**
 * useWidgetAvatarControl
 * ======================
 * Quản lý trạng thái chuyển động, cảm xúc, wander gating, và các lệnh control tag
 * của Avatar. 
 */
export function useWidgetAvatarControl(options: UseWidgetAvatarControlOptions) {
  const { isThinking, toolPresentationActive, engineRef } = options;
  const avatarControlStream = new AvatarControlTagStream();
  const wanderEnabled = ref(true);

  let avatarActionTimer: ReturnType<typeof setTimeout> | null = null;
  let avatarActionActive = false;
  let pendingAvatarAnimationId: number | null = null;
  const avatarAnimationCooldowns = new Map<number, number>();

  const restoreWanderWhenIdle = () => {
    if (!isThinking.value && !avatarActionActive && !toolPresentationActive.value) {
      engineRef.value?.setWander?.(wanderEnabled.value);
    }
  };

  const holdAvatarForAction = (durationMs: number) => {
    avatarActionActive = true;
    engineRef.value?.stopMoving?.();
    engineRef.value?.setWander?.(false);
    if (avatarActionTimer) clearTimeout(avatarActionTimer);
    avatarActionTimer = setTimeout(() => {
      avatarActionTimer = null;
      avatarActionActive = false;
      restoreWanderWhenIdle();
    }, durationMs);
  };

  const executeAvatarAction = (action: AvatarAction, registryDurationMs?: number) => {
    switch (action) {
      case 'wave':
        holdAvatarForAction(registryDurationMs ?? 1_600);
        engineRef.value?.playGesture?.('wave');
        break;
      case 'nod':
        holdAvatarForAction(registryDurationMs ?? 900);
        engineRef.value?.playGesture?.('nod');
        break;
      case 'jump':
        holdAvatarForAction(registryDurationMs ?? 1_000);
        engineRef.value?.jump?.();
        break;
      case 'come_closer':
        holdAvatarForAction(registryDurationMs ?? 4_000);
        engineRef.value?.moveTo?.(0.55, 0.9, { run: false });
        break;
      case 'step_back':
        holdAvatarForAction(registryDurationMs ?? 4_000);
        engineRef.value?.moveTo?.(0.88, 1, { run: false });
        break;
    }
  };

  const executeRegisteredAvatarAnimation = (id: number) => {
    const definition = getAvatarAnimation(id);
    if (!definition?.modelSelectable) return;

    if (isThinking.value || toolPresentationActive.value) {
      pendingAvatarAnimationId = id;
      return;
    }

    const now = Date.now();
    if ((avatarAnimationCooldowns.get(id) ?? 0) > now) return;
    avatarAnimationCooldowns.set(id, now + definition.cooldownMs);

    if (definition.kind === 'emotion') {
      engineRef.value?.setExpression?.(definition.key);
    } else if (definition.kind === 'action') {
      executeAvatarAction(definition.key as AvatarAction, definition.durationMs);
    }
  };

  watch(isThinking, (val, previous) => {
    engineRef.value?.setThinking?.(val);
    if (val && engineRef.value?.triggerMotion) {
      engineRef.value.triggerMotion();
    }
    // Đứng yên trong lúc nghĩ để người dùng còn đọc được câu trả lời đang tới;
    // chỉ đi lang thang lại khi không có hành động theo ngữ cảnh đang chạy.
    if (val) {
      engineRef.value?.stopMoving?.();
      engineRef.value?.setWander?.(false);
    } else if (previous) {
      if (pendingAvatarAnimationId !== null && !toolPresentationActive.value) {
        const id = pendingAvatarAnimationId;
        pendingAvatarAnimationId = null;
        executeRegisteredAvatarAnimation(id);
      }
      restoreWanderWhenIdle();
    }
  });

  // Đi lang thang khi rảnh
  watch(engineRef, (engine) => {
    if (!engine) return;
    engine.setThinking?.(isThinking.value);
    engine.setWander?.(wanderEnabled.value && !isThinking.value);
  });

  const cleanup = () => {
    if (avatarActionTimer) {
      clearTimeout(avatarActionTimer);
      avatarActionTimer = null;
      avatarActionActive = false;
    }
  };

  onUnmounted(cleanup);
  onDeactivated(cleanup);

  return {
    avatarControlStream,
    restoreWanderWhenIdle,
    executeAvatarAction,
    executeRegisteredAvatarAnimation,
  };
}
