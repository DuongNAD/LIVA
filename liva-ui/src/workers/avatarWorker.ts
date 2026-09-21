/**
 * avatarWorker.ts — Dedicated Web Worker for Three.js / VRM 3D Avatar Pipeline
 * =============================================================================
 * Runs Three.js WebGL2Renderer, @pixiv/three-vrm (v3.5.2), GLTFLoader,
 * 6-step humanoid additive pose loop, spring bone Verlet physics, and
 * clock-synchronized viseme lip-sync entirely off the Main UI thread.
 *
 * Guarantees steady 60 FPS isolated from DOM layout, token streaming, and GC pauses.
 */

import * as THREE from 'three';
import { GLTFLoader } from 'three/examples/jsm/loaders/GLTFLoader.js';
import { VRMLoaderPlugin, type VRM, VRMUtils } from '@pixiv/three-vrm';
import {
  useAvatarAnimation,
  type AvatarAnimationApi,
  type AvatarClipState,
  type LocomotionState,
} from '../composables/useAvatarAnimation';
import { loadMixamoAnimationSet } from '../composables/mixamoClipLoader';
import type { FaceExpressions } from '../composables/useFaceTracking';
import {
  easeOutQuad,
  lerp,
  randomBlinkInterval,
  weightedRandom,
  MAX_SACCADE_AMPLITUDE_DEG,
  SACCADE_JUMP_DURATION_S,
  SACCADE_DRIFT_HALF_LIFE_S,
  randomSaccadeInterval,
  randomSaccadeDisplacement,
} from '../utils/avatarMath';
import { TimelineQueue } from '../utils/phonemeLipSync';
import type { VisemeCue } from '../utils/speakerFrame';

// ─────────────────────────────────────────────────────────────────────────────
// 1. Types & Message Protocol
// ─────────────────────────────────────────────────────────────────────────────

export interface ScreenBoundsRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export type AvatarWorkerInbound =
  | {
      type: 'INIT';
      canvas: OffscreenCanvas;
      width: number;
      height: number;
      dpr: number;
      modelPath?: string;
      timeOrigin?: number;
    }
  | { type: 'LOAD_MODEL'; modelPath: string }
  | { type: 'LOAD_ANIMATIONS'; paths?: Partial<Record<AvatarClipState, string>> }
  | { type: 'RESIZE'; width: number; height: number; dpr: number }
  | { type: 'VISIBILITY_CHANGE'; visible: boolean }
  | { type: 'SET_ECO_MODE'; eco: boolean; demoteLevel?: 'normal' | 'eco' | 'freeze' | 'preempted' }
  | { type: 'SYNC_CLOCK'; clockOffset: number }
  | { type: 'SCHEDULE_CHUNK'; startTimeSec: number; durationSec: number }
  | { type: 'SET_VISEME_TIMELINE'; turnEpoch: number; cues: VisemeCue[] }
  | { type: 'AUDIO_RMS'; bands: Float32Array }
  | { type: 'FLUSH'; seq_id?: number }
  | { type: 'SET_LOCOMOTION'; state: LocomotionState; motionWeight?: number }
  | { type: 'SET_SCREEN_POS'; nx: number; ny: number }
  | { type: 'SET_SCREEN_POSITION'; nx: number; ny: number }
  | { type: 'SET_SCALE'; scale: number }
  | { type: 'SET_FACING'; direction: 1 | -1; turned: boolean }
  | { type: 'LOOK_AT'; yaw: number; pitch: number }
  | {
      type: 'LOOK_AT_SCREEN_POINT';
      nx: number;
      ny: number;
      windowOffset?: { x: number; y: number };
    }
  | { type: 'SET_DANGLE'; active: boolean; velocityX?: number }
  | { type: 'SET_THINKING'; active: boolean }
  | { type: 'PLAY_GESTURE'; name: 'wave' | 'nod' | 'shake' }
  | { type: 'SET_INSPECTING'; active: boolean }
  | { type: 'SET_EXPRESSION'; emotion: string; intensity?: number }
  | { type: 'UPDATE_EXPRESSIONS'; expressions: FaceExpressions }
  | { type: 'SET_FACE_TRACKING_ACTIVE'; active: boolean }
  | { type: 'START_AUTO_BLINK' }
  | { type: 'START_LIP_SYNC' }
  | { type: 'STOP_LIP_SYNC' }
  | { type: 'ON_BARGE_IN' }
  | { type: 'TRIGGER_MOTION' }
  | { type: 'START_RENDER_LOOP' }
  | { type: 'STOP_RENDER_LOOP' }
  | { type: 'DISPOSE' };

export type AvatarWorkerOutbound =
  | { type: 'READY'; format: 'vrm' | 'fbx'; hasClips: boolean }
  | { type: 'MODEL_LOADED'; format: 'vrm' | 'fbx'; hasClips: boolean }
  | { type: 'LOAD_PROGRESS'; progress: number }
  | {
      type: 'ANIMATIONS_LOADED';
      loaded: AvatarClipState[];
      failures: Partial<Record<AvatarClipState, string>>;
    }
  | { type: 'BOUNDS_UPDATED'; bounds: ScreenBoundsRect }
  | { type: 'FPS_METRICS'; currentFps: number; drawCalls: number; frameTimeMs: number }
  | { type: 'ERROR'; message: string; stack?: string };

const workerScope = self as unknown as {
  postMessage(message: AvatarWorkerOutbound): void;
  onmessage: ((e: MessageEvent<AvatarWorkerInbound>) => void) | null;
  close(): void;
};

// ─────────────────────────────────────────────────────────────────────────────
// 2. OpenSimplex 2D Noise (Inline Zero-Dep)
// ─────────────────────────────────────────────────────────────────────────────

const STRETCH_2D = (Math.sqrt(3) - 1) / 2;
const SQUISH_2D = (1 / Math.sqrt(3) - 1) / 2;
const GRADIENTS_2D = [5, 2, 2, 5, -5, 2, -2, 5, 5, -2, 2, -5, -5, -2, -2, -5];

function buildPerm(seed: number): Int16Array {
  const perm = new Int16Array(256);
  const source = new Int16Array(256);
  for (let i = 0; i < 256; i++) source[i] = i;
  seed = Math.trunc(seed * 6364136223 + 1442695040);
  for (let i = 255; i >= 0; i--) {
    seed = (seed * 25214903917 + 11) & 0xffffffffffff;
    let r = (seed + 31) % (i + 1);
    if (r < 0) r += i + 1;
    perm[i] = source[r];
    source[r] = source[i];
  }
  return perm;
}

const PERM = buildPerm(42);

function extrapolate(xsb: number, ysb: number, dx: number, dy: number): number {
  const index = (PERM[(PERM[xsb & 0xff] + ysb) & 0xff] % 8) * 2;
  return GRADIENTS_2D[index] * dx + GRADIENTS_2D[index + 1] * dy;
}

function simplex2D(x: number, y: number): number {
  const stretchOffset = (x + y) * STRETCH_2D;
  const xs = x + stretchOffset;
  const ys = y + stretchOffset;
  const xsb = Math.floor(xs);
  const ysb = Math.floor(ys);
  const squishOffset = (xsb + ysb) * SQUISH_2D;
  const dx0 = x - (xsb + squishOffset);
  const dy0 = y - (ysb + squishOffset);
  const xins = xs - xsb;
  const yins = ys - ysb;

  let value = 0;
  const attn0 = 2 - dx0 * dx0 - dy0 * dy0;
  if (attn0 > 0) {
    const attn0sq = attn0 * attn0;
    value += attn0sq * attn0sq * extrapolate(xsb, ysb, dx0, dy0);
  }

  if (xins + yins <= 1) {
    const dx1 = dx0 - 1 - SQUISH_2D;
    const dy1 = dy0 - SQUISH_2D;
    const attn1 = 2 - dx1 * dx1 - dy1 * dy1;
    if (attn1 > 0) {
      const attn1sq = attn1 * attn1;
      value += attn1sq * attn1sq * extrapolate(xsb + 1, ysb, dx1, dy1);
    }
    const dx2 = dx0 - SQUISH_2D;
    const dy2 = dy0 - 1 - SQUISH_2D;
    const attn2 = 2 - dx2 * dx2 - dy2 * dy2;
    if (attn2 > 0) {
      const attn2sq = attn2 * attn2;
      value += attn2sq * attn2sq * extrapolate(xsb, ysb + 1, dx2, dy2);
    }
  } else {
    const dx1 = dx0 - 1 - 2 * SQUISH_2D;
    const dy1 = dy0 - 1 - 2 * SQUISH_2D;
    const attn1 = 2 - dx1 * dx1 - dy1 * dy1;
    if (attn1 > 0) {
      const attn1sq = attn1 * attn1;
      value += attn1sq * attn1sq * extrapolate(xsb + 1, ysb + 1, dx1, dy1);
    }
    const dx2 = dx0 - SQUISH_2D;
    const dy2 = dy0 - 1 - SQUISH_2D;
    const attn2 = 2 - dx2 * dx2 - dy2 * dy2;
    if (attn2 > 0) {
      const attn2sq = attn2 * attn2;
      value += attn2sq * attn2sq * extrapolate(xsb, ysb + 1, dx2, dy2);
    }
    const dx3 = dx0 - 1 - SQUISH_2D;
    const dy3 = dy0 - SQUISH_2D;
    const attn3 = 2 - dx3 * dx3 - dy3 * dy3;
    if (attn3 > 0) {
      const attn3sq = attn3 * attn3;
      value += attn3sq * attn3sq * extrapolate(xsb + 1, ysb, dx3, dy3);
    }
  }
  return value / 6;
}

// ─────────────────────────────────────────────────────────────────────────────
// 3. Worker State & Three.js Hierarchy
// ─────────────────────────────────────────────────────────────────────────────

let renderer: THREE.WebGLRenderer | null = null;
const scene = new THREE.Scene();
const camera = new THREE.PerspectiveCamera(30, 400 / 700, 0.1, 20);
const avatarRoot = new THREE.Group();
scene.add(avatarRoot);

let vrm: VRM | null = null;
const animation: AvatarAnimationApi = useAvatarAnimation();
const timelineQueue = new TimelineQueue();

let viewportWidth = 400;
let viewportHeight = 700;
let devicePixelRatio = 1;
let animFrameId: number | null = null;
const clock = new THREE.Clock();
let isRunningRenderLoop = false;

// Positioning
let avatarScreenX = 0.5;
let avatarScreenY = 1.0;
let avatarScale = 0.45;
const MAX_TURN = 0.9;
let facingTarget = 0;
let facingCurrent = 0;
let dangleActive = false;
let dangleVelocityX = 0;

// Shading & Contact Shadow
let contactShadowMesh: THREE.Mesh | null = null;
let lightsInitialized = false;

function createRadialShadowTexture(): THREE.Texture {
  if (typeof OffscreenCanvas !== 'undefined') {
    const canvas = new OffscreenCanvas(128, 128);
    const ctx = canvas.getContext('2d');
    if (ctx && typeof ctx.createRadialGradient === 'function') {
      const gradient = ctx.createRadialGradient(64, 64, 0, 64, 64, 64);
      gradient.addColorStop(0, 'rgba(0, 0, 0, 0.45)');
      gradient.addColorStop(0.35, 'rgba(0, 0, 0, 0.26)');
      gradient.addColorStop(0.7, 'rgba(0, 0, 0, 0.06)');
      gradient.addColorStop(1, 'rgba(0, 0, 0, 0)');
      ctx.fillStyle = gradient;
      ctx.fillRect(0, 0, 128, 128);
      const texture = new THREE.CanvasTexture(canvas as unknown as HTMLCanvasElement);
      texture.needsUpdate = true;
      return texture;
    }
  }
  return new THREE.Texture();
}

function ensureContactShadow() {
  if (contactShadowMesh) return;
  const shadowGeo = new THREE.PlaneGeometry(0.85, 0.85);
  const shadowMat = new THREE.MeshBasicMaterial({
    map: createRadialShadowTexture(),
    transparent: true,
    opacity: 0.38,
    depthWrite: false,
  });
  contactShadowMesh = new THREE.Mesh(shadowGeo, shadowMat);
  contactShadowMesh.rotation.x = -Math.PI / 2;
  contactShadowMesh.position.set(0, 0.002, 0);
  avatarRoot.add(contactShadowMesh);
}

function ensureLighting() {
  if (lightsInitialized) return;
  const ambient = new THREE.AmbientLight(0xffffff, 0.5);
  scene.add(ambient);
  const hemi = new THREE.HemisphereLight(0xffffff, 0x444444, 0.6);
  scene.add(hemi);
  const dir = new THREE.DirectionalLight(0xffffff, 1.2);
  dir.position.set(1, 1.5, 1);
  scene.add(dir);
  const fill = new THREE.DirectionalLight(0x8888ff, 0.4);
  fill.position.set(-1, -0.5, 0.5);
  scene.add(fill);
  const rim = new THREE.DirectionalLight(0xffffff, 0.6);
  rim.position.set(0, 2.5, -2.0);
  scene.add(rim);
  lightsInitialized = true;
}

// Projection math
function getViewSize(): { width: number; height: number } {
  const distance = Math.abs(camera.position.z);
  const height = 2 * distance * Math.tan((camera.fov * Math.PI) / 360);
  return { width: height * camera.aspect, height };
}

function applyScreenPosition() {
  const view = getViewSize();
  const worldX = (avatarScreenX - 0.5) * view.width;
  const topY = camera.position.y + view.height / 2;
  const worldY = topY - avatarScreenY * view.height;
  avatarRoot.position.set(worldX, worldY, 0);
  avatarRoot.scale.set(avatarScale, avatarScale, avatarScale);
}

// ─────────────────────────────────────────────────────────────────────────────
// 4. Procedural & Blendshape States
// ─────────────────────────────────────────────────────────────────────────────

const UPPER_BODY_BONES = ['spine', 'chest', 'neck', 'head'] as const;

// Blink
let autoBlinkEnabled = true;
let blinkTimer = 0;
let nextBlinkAt = randomBlinkInterval();
let blinkPhase: 'idle' | 'closing' | 'opening' | 'closed' = 'idle';
let blinkProgress = 0;
let pendingDoubleBlink = false;

// Saccades
const saccadesEnabled = true;
let saccadeTimer = 0;
let nextSaccadeInterval = randomSaccadeInterval();
let saccadePhase: 'jump' | 'drift' = 'drift';
let saccadePhaseTimer = 0;
let saccadeStartX = 0;
let saccadeStartY = 0;
let saccadeTargetX = 0;
let saccadeTargetY = 0;
let saccadeYaw = 0;
let saccadePitch = 0;

// Spring Look-At
let currentYaw = 0;
let currentPitch = 0;
let targetYaw = 0;
let targetPitch = 0;
let faceTrackingActive = false;

// Idle & Personality
let idleTime = 0;
let microExprTimer = 0;
let nextMicroExprAt = 5 + Math.random() * 8;
let activeMicroExpr: string | null = null;
let microExprIntensity = 0;
let microExprFading = false;

// Audio & Lip-sync
let audioClockOffset = 0;
let isAudioActive = false;
let bargeInTimer = 0;
const BARGE_IN_DURATION_S = 0.35;
const BAND_EXPRESSIONS: ReadonlyArray<string> = ['aa', 'oh', 'ee', 'ih', 'ou'];
const BAND_SENSITIVITY: ReadonlyArray<number> = [1.2, 0.8, 0.6, 0.5, 0.4];
const smoothedBandRMS = new Float32Array(5);
let hasIncomingRms = false;
let proceduralLipSyncActive = false;
let proceduralLipTime = 0;

// Adaptive throttle & telemetry
const ECO_FRAME_INTERVAL_MS = 33;
let isWindowVisible = true;
let isEcoMode = false;
let demoteLevel: 'normal' | 'eco' | 'freeze' | 'preempted' = 'normal';
let lastFrameTime = 0;
let telemetryLastEmit = 0;
let telemetryFrames = 0;
let telemetryFrameTimeSum = 0;

let lastEmittedBounds: ScreenBoundsRect | null = null;
let lastBoundsCheckTime = 0;

// ─────────────────────────────────────────────────────────────────────────────
// 5. Pose Pipeline Steps (Deterministic 6-Step + Physics)
// ─────────────────────────────────────────────────────────────────────────────

function resetUpperBodyRestPose(vrmInstance: VRM) {
  const humanoid = vrmInstance.humanoid;
  if (!humanoid) return;
  for (const bone of UPPER_BODY_BONES) {
    const node = humanoid.getNormalizedBoneNode(bone);
    if (node) {
      node.rotation.x = 0;
      node.rotation.y = 0;
      node.rotation.z = 0;
    }
  }
}

function updateLocomotionUpperBody(vrmInstance: VRM) {
  const humanoid = vrmInstance.humanoid;
  if (!humanoid) return;

  const rawMotionWeight = animation.getMotionWeight();
  const motionWeight =
    Number.isFinite(rawMotionWeight) && rawMotionWeight > 0 ? Math.min(1, rawMotionWeight) : 0;
  if (motionWeight <= 0 && !dangleActive) return;

  const stridePhase = animation.getStridePhase();
  const currentState = animation.getState();
  const isRunning = currentState === 'run';

  const spine = humanoid.getNormalizedBoneNode('spine');
  if (spine) {
    if (dangleActive) {
      spine.rotation.x += 0.16;
    } else {
      spine.rotation.y += -Math.sin(stridePhase) * 0.08 * motionWeight;
      spine.rotation.x += (isRunning ? 0.2 : 0.06) * motionWeight;
    }
  }

  const head = humanoid.getNormalizedBoneNode('head');
  if (head) {
    if (dangleActive) {
      head.rotation.x -= 0.12;
    } else {
      if (spine) {
        head.rotation.x += -spine.rotation.x * 0.6;
      }
      head.rotation.y += Math.sin(stridePhase * 0.5) * 0.03 * motionWeight;
    }
  }
}

function updateIdle(delta: number) {
  if (!vrm) return;
  const safeDelta = Number.isFinite(delta) && delta > 0 ? delta : 0;
  idleTime += safeDelta;

  const spine = vrm.humanoid?.getNormalizedBoneNode('spine');
  if (spine) {
    const breathCycle = Math.sin(idleTime * Math.PI * 0.5) * 0.008;
    spine.rotation.x += breathCycle;
  }

  const head = vrm.humanoid?.getNormalizedBoneNode('head');
  if (head) {
    const swayX = simplex2D(idleTime * 0.15, 0) * 0.005 + simplex2D(idleTime * 0.4, 1.7) * 0.002;
    const swayY = simplex2D(0, idleTime * 0.12) * 0.004 + simplex2D(2.3, idleTime * 0.35) * 0.002;
    head.rotation.x += swayX;
    head.rotation.y += swayY;

    const thinkingWeight = animation.getThinkingWeight ? animation.getThinkingWeight() : 0;
    if (thinkingWeight > 0) {
      head.rotation.z += 0.1 * thinkingWeight;
      head.rotation.x += 0.04 * thinkingWeight;
      head.rotation.y += -0.03 * thinkingWeight;
    }
  }
}

function updateBlink(delta: number) {
  if (!autoBlinkEnabled || faceTrackingActive || !vrm?.expressionManager) return;
  const em = vrm.expressionManager;
  blinkTimer += delta;

  switch (blinkPhase) {
    case 'idle':
      if (blinkTimer >= nextBlinkAt) {
        blinkPhase = 'closing';
        blinkProgress = 0;
        pendingDoubleBlink = Math.random() < 0.2;
      }
      break;
    case 'closing':
      blinkProgress += delta / 0.06;
      if (blinkProgress >= 1) {
        blinkProgress = 1;
        blinkPhase = 'closed';
      }
      em.setValue('blink', easeOutQuad(blinkProgress));
      break;
    case 'closed':
      blinkProgress += delta / (0.03 + Math.random() * 0.03);
      if (blinkProgress >= 2) {
        blinkPhase = 'opening';
        blinkProgress = 0;
      }
      em.setValue('blink', 1);
      break;
    case 'opening':
      blinkProgress += delta / 0.1;
      if (blinkProgress >= 1) {
        blinkProgress = 0;
        em.setValue('blink', 0);
        if (pendingDoubleBlink) {
          pendingDoubleBlink = false;
          blinkPhase = 'closing';
          blinkTimer = nextBlinkAt - 0.15;
        } else {
          blinkPhase = 'idle';
          blinkTimer = 0;
          nextBlinkAt = randomBlinkInterval();
        }
        return;
      }
      em.setValue('blink', 1 - easeOutQuad(blinkProgress));
      break;
  }
}

function updateEyeSaccades(delta: number) {
  if (!saccadesEnabled) {
    saccadeYaw = 0;
    saccadePitch = 0;
    return;
  }
  const safeDelta = Number.isFinite(delta) && delta > 0 ? Math.min(delta, 0.1) : 0;
  if (safeDelta === 0) return;

  saccadeTimer += safeDelta;
  saccadePhaseTimer += safeDelta;

  if (saccadeTimer >= nextSaccadeInterval) {
    saccadeTimer = 0;
    nextSaccadeInterval = randomSaccadeInterval();
    saccadePhase = 'jump';
    saccadePhaseTimer = 0;
    saccadeStartX = saccadeYaw;
    saccadeStartY = saccadePitch;
    const disp = randomSaccadeDisplacement(MAX_SACCADE_AMPLITUDE_DEG);
    saccadeTargetX = disp.yaw;
    saccadeTargetY = disp.pitch;
  }

  if (saccadePhase === 'jump') {
    const progress = Math.min(saccadePhaseTimer / SACCADE_JUMP_DURATION_S, 1.0);
    const ease = progress * progress * (3 - 2 * progress);
    saccadeYaw = saccadeStartX + (saccadeTargetX - saccadeStartX) * ease;
    saccadePitch = saccadeStartY + (saccadeTargetY - saccadeStartY) * ease;
    if (progress >= 1.0) {
      saccadePhase = 'drift';
      saccadePhaseTimer = 0;
      saccadeYaw = saccadeTargetX;
      saccadePitch = saccadeTargetY;
    }
  } else {
    const driftDecay = Math.exp(-safeDelta / SACCADE_DRIFT_HALF_LIFE_S);
    saccadeYaw *= driftDecay;
    saccadePitch *= driftDecay;
    if (Math.abs(saccadeYaw) < 0.0001) saccadeYaw = 0;
    if (Math.abs(saccadePitch) < 0.0001) saccadePitch = 0;
  }
}

function updateSpringLookAt(delta: number) {
  if (!vrm?.lookAt?.applier) return;
  const safeDelta = Number.isFinite(delta) && delta > 0 ? delta : 0;
  const springFactor = 1 - Math.pow(0.001, safeDelta);

  currentYaw = lerp(currentYaw, targetYaw, springFactor);
  currentPitch = lerp(currentPitch, targetPitch, springFactor);

  const finalYaw = currentYaw + saccadeYaw;
  const finalPitch = currentPitch + saccadePitch;
  vrm.lookAt.applier.applyYawPitch(finalYaw, finalPitch);
}

function updateAudioLipSync(delta: number) {
  if (bargeInTimer > 0 || !vrm?.expressionManager) return;
  const em = vrm.expressionManager;

  const epochNowSec = ((performance.timeOrigin || 0) + performance.now()) / 1000;
  const currentAudioTime = epochNowSec + audioClockOffset;
  const phonemeViseme = timelineQueue.currentViseme(currentAudioTime);

  if (phonemeViseme !== null) {
    isAudioActive = true;
    for (const expr of BAND_EXPRESSIONS) {
      if (phonemeViseme === 'nil') {
        em.setValue(expr, 0);
      } else if (expr === phonemeViseme) {
        em.setValue(expr, 0.85);
      } else {
        em.setValue(expr, 0);
      }
    }
  } else if (hasIncomingRms) {
    isAudioActive = true;
    for (let band = 0; band < BAND_EXPRESSIONS.length; band++) {
      const val = Math.min(smoothedBandRMS[band] * BAND_SENSITIVITY[band], 1.0);
      em.setValue(BAND_EXPRESSIONS[band], val);
    }
  } else if (proceduralLipSyncActive) {
    isAudioActive = true;
    proceduralLipTime += delta;
    const mouthOpen = (Math.sin(proceduralLipTime * 12) * 0.5 + 0.5) * 0.6;
    em.setValue('aa', mouthOpen);
  } else if (isAudioActive) {
    isAudioActive = false;
    for (const expr of BAND_EXPRESSIONS) {
      em.setValue(expr, 0);
    }
  }
}

function updateMicroExpressions(delta: number) {
  if (!vrm?.expressionManager || isAudioActive || bargeInTimer > 0 || faceTrackingActive) {
    return;
  }
  const em = vrm.expressionManager;
  microExprTimer += delta;

  if (!activeMicroExpr) {
    if (microExprTimer >= nextMicroExprAt) {
      activeMicroExpr = weightedRandom(['happy', 'relaxed', 'surprised'], [0.5, 0.35, 0.15]);
      microExprIntensity = 0;
      microExprFading = false;
      microExprTimer = 0;
    }
  } else {
    if (!microExprFading) {
      microExprIntensity += delta / 0.4;
      const targetIntensity = 0.35;
      if (microExprIntensity >= targetIntensity) {
        microExprIntensity = targetIntensity;
        microExprFading = true;
        microExprTimer = 0;
      }
      em.setValue(activeMicroExpr, easeOutQuad(microExprIntensity));
    } else {
      if (microExprTimer < 1.0) {
        em.setValue(activeMicroExpr, microExprIntensity);
      } else {
        microExprIntensity -= delta / 0.6;
        if (microExprIntensity <= 0) {
          em.setValue(activeMicroExpr, 0);
          activeMicroExpr = null;
          microExprTimer = 0;
          nextMicroExprAt = 5 + Math.random() * 10;
        } else {
          em.setValue(activeMicroExpr, easeOutQuad(microExprIntensity));
        }
      }
    }
  }
}

function updateBargeIn(delta: number) {
  if (bargeInTimer <= 0 || !vrm?.expressionManager) return;
  bargeInTimer -= delta;
  const em = vrm.expressionManager;

  for (const expr of BAND_EXPRESSIONS) {
    em.setValue(expr, 0);
  }

  if (bargeInTimer <= 0) {
    bargeInTimer = 0;
    em.setValue('surprised', 0);
  } else {
    const progress = Math.max(0, bargeInTimer / BARGE_IN_DURATION_S);
    em.setValue('surprised', 0.45 * progress);
  }
}

// ─────────────────────────────────────────────────────────────────────────────
// 6. Interactive Screen Bounds Calculation
// ─────────────────────────────────────────────────────────────────────────────

function computeScreenBounds(): ScreenBoundsRect | null {
  if (!vrm?.scene) return null;
  avatarRoot.updateMatrixWorld(true);

  const box = new THREE.Box3().setFromObject(vrm.scene);
  if (!Number.isFinite(box.min.x) || !Number.isFinite(box.max.x)) return null;

  let minX = Infinity;
  let minY = Infinity;
  let maxX = -Infinity;
  let maxY = -Infinity;

  const corner = new THREE.Vector3();
  for (let i = 0; i < 8; i++) {
    corner.set(
      i & 1 ? box.max.x : box.min.x,
      i & 2 ? box.max.y : box.min.y,
      i & 4 ? box.max.z : box.min.z
    );
    corner.project(camera);
    const px = (corner.x * 0.5 + 0.5) * viewportWidth;
    const py = (-corner.y * 0.5 + 0.5) * viewportHeight;
    if (px < minX) minX = px;
    if (px > maxX) maxX = px;
    if (py < minY) minY = py;
    if (py > maxY) maxY = py;
  }

  const clampedMinX = Math.max(minX, 0);
  const clampedMinY = Math.max(minY, 0);
  const clampedMaxX = Math.min(maxX, viewportWidth);
  const clampedMaxY = Math.min(maxY, viewportHeight);
  if (clampedMaxX <= clampedMinX || clampedMaxY <= clampedMinY) return null;

  return {
    x: Math.round(clampedMinX),
    y: Math.round(clampedMinY),
    width: Math.round(clampedMaxX - clampedMinX),
    height: Math.round(clampedMaxY - clampedMinY),
  };
}

// ─────────────────────────────────────────────────────────────────────────────
// 7. Master Render Loop & Adaptive Throttle
// ─────────────────────────────────────────────────────────────────────────────

function renderLoop(now: number) {
  if (!isRunningRenderLoop) return;
  animFrameId = requestAnimationFrame(renderLoop);

  // Level 0: Freeze
  if (demoteLevel === 'freeze' || demoteLevel === 'preempted') return;

  // Adaptive throttle
  const isAvatarIdle =
    !dangleActive &&
    !isAudioActive &&
    animation.getState() === 'idle' &&
    animation.getMotionWeight() <= 0.01 &&
    facingCurrent === facingTarget;

  const throttleInterval = isEcoMode
    ? ECO_FRAME_INTERVAL_MS
    : !isWindowVisible
      ? 66
      : isAvatarIdle
        ? ECO_FRAME_INTERVAL_MS
        : 0;

  if (throttleInterval > 0 && lastFrameTime > 0 && now - lastFrameTime < throttleInterval) {
    return;
  }
  lastFrameTime = now;

  const frameStart = performance.now();
  const rawDelta = clock.getDelta();
  const delta = Math.min(rawDelta, 0.1);

  // Dynamic turn banking & pendulum dangle
  if (dangleActive) {
    const targetBank = Math.max(-0.35, Math.min(0.35, -dangleVelocityX * 0.0006));
    avatarRoot.rotation.z = lerp(avatarRoot.rotation.z, targetBank, 1 - Math.pow(0.005, delta));
  } else if (facingCurrent !== facingTarget) {
    const turnFactor = 1 - Math.pow(0.005, delta);
    const prevFacing = facingCurrent;
    facingCurrent = lerp(facingCurrent, facingTarget, turnFactor);
    if (Math.abs(facingCurrent - facingTarget) < 0.001) {
      facingCurrent = facingTarget;
    }
    avatarRoot.rotation.y = facingCurrent;
    const turnVelocity = (facingCurrent - prevFacing) / (delta || 0.016);
    const targetBank = Math.max(-0.08, Math.min(0.08, -turnVelocity * 0.015));
    avatarRoot.rotation.z = lerp(avatarRoot.rotation.z, targetBank, turnFactor);
  } else {
    avatarRoot.rotation.z = lerp(avatarRoot.rotation.z, 0, 1 - Math.pow(0.01, delta));
  }

  // ── Unified 6-Step Humanoid Pose Execution Order ──
  if (vrm) {
    // Step 1: Base pose evaluation (locomotion / clip + FootPlantIK)
    animation.update(vrm, delta);

    // Step 2: Rest pose reset (spine, chest, neck, head)
    resetUpperBodyRestPose(vrm);

    // Step 3: Locomotion upper-body procedural layer (motionWeight > 0)
    updateLocomotionUpperBody(vrm);

    // Step 4: Idle procedural layer (breathing sine + OpenSimplex noise sway)
    updateIdle(delta);

    // Step 5: Facial blend shapes, eye saccades, spring look-at, micro-expressions, lip-sync
    updateBlink(delta);
    updateAudioLipSync(delta);
    updateEyeSaccades(delta);
    updateSpringLookAt(delta);
    updateMicroExpressions(delta);
    updateBargeIn(delta);

    // Step 6: VRM secondary animation / spring bone physics (Verlet sub-stepping)
    const physicsSteps = Math.min(2, Math.max(1, Math.ceil(delta / (1 / 60))));
    const physicsDelta = delta / physicsSteps;
    for (let step = 0; step < physicsSteps; step += 1) {
      vrm.update(physicsDelta);
    }
  }

  // Step 7: Offscreen WebGL render
  if (renderer) {
    renderer.render(scene, camera);
  }

  const frameDuration = performance.now() - frameStart;
  telemetryFrames++;
  telemetryFrameTimeSum += frameDuration;

  // Throttled Screen Bounds emission (every 100ms)
  if (now - lastBoundsCheckTime >= 100) {
    lastBoundsCheckTime = now;
    const currentBounds = computeScreenBounds();
    if (currentBounds) {
      const hasChanged =
        !lastEmittedBounds ||
        Math.abs(currentBounds.x - lastEmittedBounds.x) > 2 ||
        Math.abs(currentBounds.y - lastEmittedBounds.y) > 2 ||
        Math.abs(currentBounds.width - lastEmittedBounds.width) > 2 ||
        Math.abs(currentBounds.height - lastEmittedBounds.height) > 2;

      if (hasChanged) {
        lastEmittedBounds = currentBounds;
        workerScope.postMessage({
          type: 'BOUNDS_UPDATED',
          bounds: currentBounds,
        });
      }
    }
  }

  // FPS Telemetry emission (1 Hz)
  if (now - telemetryLastEmit >= 1000) {
    const elapsedSec = (now - telemetryLastEmit) / 1000;
    const currentFps = Math.round(telemetryFrames / elapsedSec);
    const avgFrameTimeMs = telemetryFrames > 0 ? telemetryFrameTimeSum / telemetryFrames : 0;
    const drawCalls =
      (renderer as unknown as { info?: { render?: { calls: number } } })?.info?.render?.calls ?? 0;

    workerScope.postMessage({
      type: 'FPS_METRICS',
      currentFps,
      drawCalls,
      frameTimeMs: Math.round(avgFrameTimeMs * 100) / 100,
    });

    telemetryLastEmit = now;
    telemetryFrames = 0;
    telemetryFrameTimeSum = 0;
  }
}

// ─────────────────────────────────────────────────────────────────────────────
// 8. Model Loading & Deep Disposal
// ─────────────────────────────────────────────────────────────────────────────

async function loadVRMModel(modelPath: string): Promise<void> {
  const loader = new GLTFLoader();
  loader.register(
    (parser) =>
      new VRMLoaderPlugin(parser, {
        autoUpdateHumanBones: true,
      })
  );

  return new Promise((resolve, reject) => {
    loader.load(
      modelPath,
      (gltf) => {
        const loadedVRM = gltf.userData.vrm as VRM;
        if (!loadedVRM) {
          const err = new Error('GLTF missing VRM payload');
          workerScope.postMessage({ type: 'ERROR', message: err.message });
          reject(err);
          return;
        }

        if (vrm) {
          deepDispose(vrm.scene);
          avatarRoot.remove(vrm.scene);
          vrm = null;
        }

        VRMUtils.removeUnnecessaryVertices(gltf.scene);
        VRMUtils.combineSkeletons(gltf.scene);
        VRMUtils.rotateVRM0(loadedVRM);

        if (loadedVRM.lookAt) {
          loadedVRM.lookAt.target = undefined;
        }

        avatarRoot.add(loadedVRM.scene);
        vrm = loadedVRM;
        applyScreenPosition();

        workerScope.postMessage({ type: 'READY', format: 'vrm', hasClips: false });
        workerScope.postMessage({
          type: 'MODEL_LOADED',
          format: 'vrm',
          hasClips: false,
        });
        resolve();
      },
      (progressEvent) => {
        if (progressEvent.total > 0) {
          const pct = Math.round((progressEvent.loaded / progressEvent.total) * 100);
          workerScope.postMessage({ type: 'LOAD_PROGRESS', progress: pct });
        }
      },
      (err) => {
        const errorMsg = err instanceof Error ? err.message : String(err);
        workerScope.postMessage({
          type: 'ERROR',
          message: `Failed to load VRM: ${errorMsg}`,
        });
        reject(err);
      }
    );
  });
}

function deepDispose(root: THREE.Object3D) {
  root.traverse((object) => {
    const obj = object as THREE.Object3D & {
      geometry?: { dispose: () => void };
      material?: THREE.Material | THREE.Material[];
      skeleton?: { dispose: () => void };
    };
    if (obj.geometry) obj.geometry.dispose();
    if (obj.material) {
      const mats = Array.isArray(obj.material) ? obj.material : [obj.material];
      mats.forEach((m) => {
        Object.values(m).forEach((val) => {
          if (
            val &&
            typeof val === 'object' &&
            'isTexture' in val &&
            typeof (val as { dispose?: () => void }).dispose === 'function'
          ) {
            (val as { dispose: () => void }).dispose();
          }
        });
        m.dispose();
      });
    }
    if (obj.skeleton) obj.skeleton.dispose();
  });
}

function disposeAll() {
  isRunningRenderLoop = false;
  if (animFrameId !== null) {
    cancelAnimationFrame(animFrameId);
    animFrameId = null;
  }
  if (vrm) {
    deepDispose(vrm.scene);
    avatarRoot.remove(vrm.scene);
    vrm = null;
  }
  if (contactShadowMesh) {
    avatarRoot.remove(contactShadowMesh);
    contactShadowMesh.geometry.dispose();
    const mat = contactShadowMesh.material as THREE.MeshBasicMaterial;
    mat.map?.dispose();
    mat.dispose();
    contactShadowMesh = null;
  }
  if (renderer) {
    renderer.dispose();
    renderer.forceContextLoss();
    renderer = null;
  }
  timelineQueue.reset();
  workerScope.close();
}

// ─────────────────────────────────────────────────────────────────────────────
// 9. Inbound Message Router
// ─────────────────────────────────────────────────────────────────────────────

workerScope.onmessage = async (e: MessageEvent<AvatarWorkerInbound>) => {
  const msg = e.data;
  if (!msg) return;

  switch (msg.type) {
    case 'INIT': {
      try {
        const { canvas, width, height, dpr, modelPath } = msg;
        viewportWidth = width;
        viewportHeight = height;
        devicePixelRatio = dpr;

        renderer = new THREE.WebGLRenderer({
          canvas: canvas as unknown as HTMLCanvasElement,
          alpha: true,
          antialias: true,
        });
        renderer.setClearColor(0x000000, 0);
        renderer.setPixelRatio(devicePixelRatio);
        (renderer.setSize as (w: number, h: number, updateStyle?: boolean) => void)(
          viewportWidth,
          viewportHeight,
          false
        );
        (renderer as unknown as Record<string, unknown>).outputColorSpace = (
          THREE as unknown as { SRGBColorSpace: string }
        ).SRGBColorSpace;

        camera.aspect = viewportWidth / viewportHeight;
        camera.position.set(0, 1.05, 5.8);
        camera.lookAt(0, 1.0, 0);
        camera.updateProjectionMatrix();

        ensureLighting();
        ensureContactShadow();
        applyScreenPosition();

        if (modelPath) {
          await loadVRMModel(modelPath);
        } else {
          workerScope.postMessage({ type: 'READY', format: 'vrm', hasClips: false });
        }

        // Start render loop
        isRunningRenderLoop = true;
        lastFrameTime = performance.now();
        telemetryLastEmit = performance.now();
        renderLoop(lastFrameTime);
      } catch (err) {
        workerScope.postMessage({
          type: 'ERROR',
          message: `Worker INIT failed: ${err instanceof Error ? err.message : String(err)}`,
          stack: err instanceof Error ? err.stack : undefined,
        });
      }
      break;
    }

    case 'LOAD_MODEL': {
      try {
        await loadVRMModel(msg.modelPath);
      } catch (err) {
        workerScope.postMessage({
          type: 'ERROR',
          message: `LOAD_MODEL failed: ${err instanceof Error ? err.message : String(err)}`,
        });
      }
      break;
    }

    case 'LOAD_ANIMATIONS': {
      if (!vrm) {
        workerScope.postMessage({
          type: 'ANIMATIONS_LOADED',
          loaded: [],
          failures: { idle: 'VRM model not loaded yet' },
        });
        break;
      }
      try {
        const result = await loadMixamoAnimationSet(vrm, msg.paths);
        const loadedKeys = Object.keys(result.clips) as AvatarClipState[];
        loadedKeys.forEach((key) => {
          const clip = result.clips[key];
          if (clip) {
            animation.registerClip(key, clip);
          }
        });
        workerScope.postMessage({
          type: 'ANIMATIONS_LOADED',
          loaded: loadedKeys,
          failures: result.failures,
        });
      } catch (animErr) {
        workerScope.postMessage({
          type: 'ANIMATIONS_LOADED',
          loaded: [],
          failures: {
            idle: animErr instanceof Error ? animErr.message : String(animErr),
          },
        });
      }
      break;
    }

    case 'RESIZE': {
      const { width, height, dpr } = msg;
      if (width <= 0 || height <= 0) return;
      viewportWidth = width;
      viewportHeight = height;
      devicePixelRatio = dpr;
      camera.aspect = width / height;
      camera.updateProjectionMatrix();
      if (renderer) {
        renderer.setPixelRatio(dpr);
        (renderer.setSize as (w: number, h: number, updateStyle?: boolean) => void)(
          width,
          height,
          false
        );
      }
      applyScreenPosition();
      break;
    }

    case 'VISIBILITY_CHANGE': {
      isWindowVisible = msg.visible;
      break;
    }

    case 'SET_ECO_MODE': {
      isEcoMode = msg.eco;
      if (msg.demoteLevel) demoteLevel = msg.demoteLevel;
      break;
    }

    case 'SET_LOCOMOTION': {
      animation.setState(msg.state);
      if (typeof msg.motionWeight === 'number') {
        animation.setMotionWeight(msg.motionWeight);
      }
      break;
    }

    case 'SET_SCREEN_POS':
    case 'SET_SCREEN_POSITION': {
      avatarScreenX = Math.min(Math.max(msg.nx, 0), 1);
      avatarScreenY = Math.min(Math.max(msg.ny, 0), 1);
      applyScreenPosition();
      break;
    }

    case 'SET_SCALE': {
      avatarScale = Math.min(Math.max(msg.scale, 0.05), 4);
      applyScreenPosition();
      break;
    }

    case 'SET_FACING': {
      facingTarget = msg.turned ? msg.direction * MAX_TURN : 0;
      break;
    }

    case 'LOOK_AT': {
      targetYaw = msg.yaw;
      targetPitch = msg.pitch;
      break;
    }

    case 'LOOK_AT_SCREEN_POINT': {
      const offsetX = msg.windowOffset?.x ?? 0;
      const offsetY = msg.windowOffset?.y ?? 0;
      const targetX = msg.windowOffset ? msg.nx - offsetX : Math.min(Math.max(msg.nx, 0), 1);
      const targetY = msg.windowOffset ? msg.ny - offsetY : Math.min(Math.max(msg.ny, 0), 1);
      const dx = targetX - avatarScreenX;
      const dy = targetY - avatarScreenY;
      const dir: 1 | -1 = dx >= 0 ? 1 : -1;
      facingTarget = dir * MAX_TURN;
      targetYaw = Math.round(Math.min(Math.max(dx * 90, -45), 45) * 1000) / 1000;
      targetPitch = Math.round(Math.min(Math.max(dy * 70, -35), 35) * 1000) / 1000;
      break;
    }

    case 'SET_DANGLE': {
      dangleActive = msg.active;
      dangleVelocityX = msg.velocityX ?? 0;
      if (msg.active) {
        animation.setMotionWeight(1);
        animation.setState('dangle');
        if (vrm?.expressionManager) {
          vrm.expressionManager.setValue('surprised', 0.85);
          vrm.expressionManager.setValue('oh', 0.4);
          vrm.expressionManager.setValue('aa', 0.15);
        }
      } else {
        if (animation.getState() === 'dangle') {
          animation.setState('idle');
          animation.setMotionWeight(0);
        }
        if (vrm?.expressionManager) {
          vrm.expressionManager.setValue('surprised', 0);
          vrm.expressionManager.setValue('oh', 0);
          vrm.expressionManager.setValue('aa', 0);
        }
      }
      break;
    }

    case 'SET_THINKING': {
      animation.setThinking(msg.active);
      if (msg.active && !faceTrackingActive) {
        targetYaw = 0;
        targetPitch = 4;
      }
      break;
    }

    case 'PLAY_GESTURE': {
      animation.playGesture(msg.name);
      break;
    }

    case 'SET_INSPECTING': {
      animation.setInspecting(msg.active);
      break;
    }

    case 'SET_EXPRESSION': {
      if (vrm?.expressionManager) {
        vrm.expressionManager.setValue(msg.emotion, msg.intensity ?? 0.5);
      }
      break;
    }

    case 'UPDATE_EXPRESSIONS': {
      if (!vrm?.expressionManager) break;
      const em = vrm.expressionManager;
      const expressions = msg.expressions;
      const smooth = 0.3;

      if (expressions.happy > 0.15) {
        const current = em.getValue('happy') ?? 0;
        em.setValue('happy', lerp(current, expressions.happy, smooth));
      } else {
        em.setValue('happy', lerp(em.getValue('happy') ?? 0, 0, smooth));
      }

      if (expressions.surprised > 0.2) {
        const current = em.getValue('surprised') ?? 0;
        em.setValue('surprised', lerp(current, expressions.surprised, smooth));
      } else {
        em.setValue('surprised', lerp(em.getValue('surprised') ?? 0, 0, smooth));
      }

      if (expressions.angry > 0.2) {
        const current = em.getValue('angry') ?? 0;
        em.setValue('angry', lerp(current, expressions.angry, smooth));
      } else {
        em.setValue('angry', lerp(em.getValue('angry') ?? 0, 0, smooth));
      }

      if (faceTrackingActive) {
        em.setValue('blink', expressions.blink);
      }

      if (expressions.mouthOpen > 0.1) {
        em.setValue('aa', expressions.mouthOpen * 0.8);
      } else {
        em.setValue('aa', lerp(em.getValue('aa') ?? 0, 0, smooth));
      }
      break;
    }

    case 'SET_FACE_TRACKING_ACTIVE': {
      faceTrackingActive = msg.active;
      if (msg.active) {
        blinkPhase = 'idle';
      } else {
        autoBlinkEnabled = true;
        if (vrm?.expressionManager) {
          const em = vrm.expressionManager;
          em.setValue('happy', 0);
          em.setValue('surprised', 0);
          em.setValue('angry', 0);
          em.setValue('aa', 0);
        }
        targetYaw = 0;
        targetPitch = 0;
      }
      break;
    }

    case 'START_AUTO_BLINK': {
      autoBlinkEnabled = true;
      blinkTimer = 0;
      blinkPhase = 'idle';
      nextBlinkAt = randomBlinkInterval();
      break;
    }

    case 'START_LIP_SYNC': {
      proceduralLipSyncActive = true;
      proceduralLipTime = 0;
      break;
    }

    case 'STOP_LIP_SYNC': {
      proceduralLipSyncActive = false;
      if (vrm?.expressionManager) {
        vrm.expressionManager.setValue('aa', 0);
      }
      break;
    }

    case 'ON_BARGE_IN': {
      proceduralLipSyncActive = false;
      timelineQueue.reset();
      hasIncomingRms = false;
      smoothedBandRMS.fill(0);
      bargeInTimer = BARGE_IN_DURATION_S;
      targetYaw = 0;
      targetPitch = 0;
      if (vrm?.expressionManager) {
        vrm.expressionManager.setValue('surprised', 0.45);
        for (const expr of BAND_EXPRESSIONS) {
          vrm.expressionManager.setValue(expr, 0);
        }
      }
      break;
    }

    case 'TRIGGER_MOTION': {
      if (!vrm?.expressionManager) break;
      const em = vrm.expressionManager;
      const options = ['happy', 'surprised', 'relaxed'];
      const weights = [0.45, 0.3, 0.25];
      const expr = weightedRandom(options, weights);
      const peakIntensity = 0.4 + Math.random() * 0.4;
      em.setValue(expr, peakIntensity);
      break;
    }

    case 'START_RENDER_LOOP': {
      if (!isRunningRenderLoop) {
        isRunningRenderLoop = true;
        lastFrameTime = performance.now();
        renderLoop(lastFrameTime);
      }
      break;
    }

    case 'STOP_RENDER_LOOP': {
      isRunningRenderLoop = false;
      if (animFrameId !== null) {
        cancelAnimationFrame(animFrameId);
        animFrameId = null;
      }
      break;
    }

    case 'SYNC_CLOCK': {
      audioClockOffset = msg.clockOffset;
      break;
    }

    case 'SCHEDULE_CHUNK': {
      timelineQueue.noteChunkScheduled(msg.startTimeSec, msg.durationSec);
      break;
    }

    case 'SET_VISEME_TIMELINE': {
      timelineQueue.setPending(msg.cues);
      break;
    }

    case 'AUDIO_RMS': {
      hasIncomingRms = true;
      for (let i = 0; i < 5 && i < msg.bands.length; i++) {
        const rms = msg.bands[i];
        const smooth = rms > smoothedBandRMS[i] ? 0.8 : 0.25;
        smoothedBandRMS[i] = lerp(smoothedBandRMS[i], rms, smooth);
      }
      break;
    }

    case 'FLUSH': {
      timelineQueue.reset();
      hasIncomingRms = false;
      smoothedBandRMS.fill(0);
      bargeInTimer = BARGE_IN_DURATION_S;
      targetYaw = 0;
      targetPitch = 0;
      if (vrm?.expressionManager) {
        vrm.expressionManager.setValue('surprised', 0.45);
        for (const expr of BAND_EXPRESSIONS) {
          vrm.expressionManager.setValue(expr, 0);
        }
      }
      break;
    }

    case 'DISPOSE': {
      disposeAll();
      break;
    }
  }
};
