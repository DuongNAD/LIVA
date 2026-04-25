/**
 * use3DModel.ts — Vue Composable for 3D Avatar (VRM + FBX)
 * ==========================================================
 * Handles: VRM and FBX model loading, auto-blink, lip-sync, lookAt, Deep Dispose.
 * Face Tracking: updateLookAt() + updateExpressions() for MediaPipe (VRM only).
 * Idle: OpenSimplex noise-based micro-sway + breathing.
 * FBX: Auto-scale/center via Box3, AnimationMixer for embedded clips.
 */
import * as THREE from "three";
import { GLTFLoader } from "three/examples/jsm/loaders/GLTFLoader.js";
import { FBXLoader } from "three/examples/jsm/loaders/FBXLoader.js";
import { VRMLoaderPlugin, VRM, VRMUtils } from "@pixiv/three-vrm";
import { ref, type Ref } from "vue";
import type { FaceExpressions } from "./useFaceTracking";

// ═══════════════════════════════════════════
//  OpenSimplex 2D Noise (inline, zero-dep)
//  Value noise with smooth gradients — never repeats
// ═══════════════════════════════════════════
const STRETCH_2D = (Math.sqrt(3) - 1) / 2;
const SQUISH_2D = (1 / Math.sqrt(3) - 1) / 2;

// Deterministic gradient table (seeded via permutation)
const GRADIENTS_2D = [
  5, 2, 2, 5, -5, 2, -2, 5,
  5, -2, 2, -5, -5, -2, -2, -5,
];

// Generate permutation table with seed
function buildPerm(seed: number): Int16Array {
  const perm = new Int16Array(256);
  const source = new Int16Array(256);
  for (let i = 0; i < 256; i++) source[i] = i;
  seed = Math.trunc(seed * 6364136223 + 1442695040);
  for (let i = 255; i >= 0; i--) {
    seed = (seed * 25214903917 + 11) & 0xffffffffffff;
    let r = ((seed + 31) % (i + 1));
    if (r < 0) r += i + 1;
    perm[i] = source[r];
    source[r] = source[i];
  }
  return perm;
}

const PERM = buildPerm(42); // Fixed seed for consistency

/**
 * 2D OpenSimplex noise. Returns value in [-1, 1].
 */
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

  // Contribution (0,0)
  const attn0 = 2 - dx0 * dx0 - dy0 * dy0;
  if (attn0 > 0) {
    const attn0sq = attn0 * attn0;
    value += attn0sq * attn0sq * extrapolate(xsb, ysb, dx0, dy0);
  }

  // Contribution (1,0) or (0,1)
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

function extrapolate(xsb: number, ysb: number, dx: number, dy: number): number {
  const index = (PERM[(PERM[xsb & 0xff] + ysb) & 0xff] % 8) * 2;
  return GRADIENTS_2D[index] * dx + GRADIENTS_2D[index + 1] * dy;
}

export type ModelFormat = 'vrm' | 'fbx' | null;

export interface Use3DModelReturn {
  vrm: Ref<VRM | null>;
  currentModelFormat: Ref<ModelFormat>;
  scene: THREE.Scene;
  camera: THREE.PerspectiveCamera;
  renderer: THREE.WebGLRenderer | null;
  loadModel: (path: string, onProgress?: (pct: number) => void) => Promise<void>;
  initRenderer: (canvas: HTMLCanvasElement, width: number, height: number) => void;
  startRenderLoop: () => void;
  stopRenderLoop: () => void;
  startAutoBlink: () => void;
  startLipSync: () => void;
  stopLipSync: () => void;
  triggerMotion: () => void;
  updateLookAt: (yaw: number, pitch: number) => void;
  updateExpressions: (expressions: FaceExpressions) => void;
  setFaceTrackingActive: (active: boolean) => void;
  dispose: () => void;
}

/**
 * Deep Dispose — Giải phóng VRAM hoàn toàn
 * Gọi khi swap model hoặc unmount component.
 * Works for BOTH VRM and FBX scenes.
 */
function deepDispose(root: THREE.Object3D) {
  root.traverse((object: any) => {
    // Dispose geometry
    if (object.geometry) {
      object.geometry.dispose();
    }

    // Dispose materials + textures
    if (object.material) {
      const materials = Array.isArray(object.material) ? object.material : [object.material];
      materials.forEach((mat: any) => {
        // Quét tất cả texture maps (diffuse, normal, emissive, etc.)
        Object.values(mat).forEach((val: any) => {
          if (val && typeof val === 'object' && 'isTexture' in val) {
            val.dispose();
          }
        });
        mat.dispose();
      });
    }

    // Dispose skeleton (FBX models often have these)
    if (object.skeleton) {
      object.skeleton.dispose();
    }
  });
}

export function use3DModel(): Use3DModelReturn {
  const vrm = ref<VRM | null>(null);
  const currentModelFormat = ref<ModelFormat>(null);
  const scene = new THREE.Scene();
  const camera = new THREE.PerspectiveCamera(30, 500 / 700, 0.1, 20);
  let renderer: THREE.WebGLRenderer | null = null;
  let animFrameId: number | null = null;
  let clock = new THREE.Clock();

  // FBX state
  let fbxModel: THREE.Group | null = null;
  let mixer: THREE.AnimationMixer | null = null;

  // Blink state
  let blinkTimer = 0;
  let nextBlinkAt = randomBlinkInterval();
  let blinkPhase: 'idle' | 'closing' | 'opening' | 'closed' = 'idle';
  let blinkProgress = 0;
  let pendingDoubleBlink = false;
  let isBlinking = false;

  // Lip-sync state
  let lipSyncActive = false;
  let lipTime = 0;

  // Idle animation state
  let idleTime = 0;
  let microExprTimer = 0;
  let nextMicroExprAt = 5 + Math.random() * 8; // 5-13s
  let activeMicroExpr: string | null = null;
  let microExprIntensity = 0;
  let microExprFading = false;

  // Spring-damped LookAt state
  let currentYaw = 0;
  let currentPitch = 0;
  let targetYaw = 0;
  let targetPitch = 0;

  // Face tracking state — when active, disables auto-blink (real blinks take over)
  let faceTrackingActive = false;

  // ═══════════════════════════════════════════
  //  Renderer Init
  // ═══════════════════════════════════════════
  function initRenderer(canvas: HTMLCanvasElement, width: number, height: number) {
    renderer = new THREE.WebGLRenderer({
      canvas,
      alpha: true,    // Transparent background
      antialias: true,
    });
    renderer.setClearColor(0x000000, 0);  // Fully transparent
    renderer.setSize(width, height);
    renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));

    // Camera position
    camera.aspect = width / height;
    camera.position.set(0, 1, 3.5);
    camera.lookAt(0, 1, 0);
    camera.updateProjectionMatrix();

    // Lighting — Enhanced for BOTH MToon (VRM) and PBR (FBX)
    const ambientLight = new THREE.AmbientLight(0xffffff, 0.5);
    scene.add(ambientLight);

    const hemiLight = new THREE.HemisphereLight(0xffffff, 0x444444, 0.6);
    scene.add(hemiLight);

    const dirLight = new THREE.DirectionalLight(0xffffff, 1.2);
    dirLight.position.set(1, 1.5, 1);
    scene.add(dirLight);

    // Subtle fill light from below (prevents dark underside on FBX PBR)
    const fillLight = new THREE.DirectionalLight(0x8888ff, 0.4);
    fillLight.position.set(-1, -0.5, 0.5);
    scene.add(fillLight);
  }

  // ═══════════════════════════════════════════
  //  Model Loading
  // ═══════════════════════════════════════════
  /**
   * Dispose previous model (VRM or FBX) — prevents VRAM leak on swap.
   */
  function disposePreviousModel() {
    if (vrm.value) {
      deepDispose(vrm.value.scene);
      scene.remove(vrm.value.scene);
      vrm.value = null;
    }
    if (fbxModel) {
      deepDispose(fbxModel);
      scene.remove(fbxModel);
      fbxModel = null;
    }
    if (mixer) {
      mixer.stopAllAction();
      mixer = null;
    }
    currentModelFormat.value = null;
  }

  /**
   * Auto-scale and center any 3D object using its bounding box.
   * Handles arbitrary FBX scales (0.01, 1, 100) from Blender/Maya/Mixamo.
   */
  function autoScaleAndCenter(object: THREE.Object3D, targetHeight = 1.7) {
    const box = new THREE.Box3().setFromObject(object);
    const size = box.getSize(new THREE.Vector3());
    const center = box.getCenter(new THREE.Vector3());

    // Scale to target height (roughly human-sized for avatar view)
    const maxDim = Math.max(size.x, size.y, size.z);
    if (maxDim > 0) {
      const scale = targetHeight / maxDim;
      object.scale.multiplyScalar(scale);
    }

    // Re-center: move pivot so model stands at origin
    const boxAfter = new THREE.Box3().setFromObject(object);
    const centerAfter = boxAfter.getCenter(new THREE.Vector3());
    object.position.sub(centerAfter);
    // Place feet on ground (Y=0)
    const boxFinal = new THREE.Box3().setFromObject(object);
    object.position.y -= boxFinal.min.y;
  }

  async function loadModel(path: string, onProgress?: (pct: number) => void) {
    // Dispose previous model (VRM or FBX) — critical for memory
    disposePreviousModel();

    const ext = path.split('.').pop()?.toLowerCase();

    if (ext === 'fbx') {
      await loadFBX(path, onProgress);
    } else {
      await loadVRM(path, onProgress);
    }
  }

  /** Load VRM model (original logic preserved) */
  function loadVRM(path: string, onProgress?: (pct: number) => void): Promise<void> {
    const loader = new GLTFLoader();
    loader.register((parser) => new VRMLoaderPlugin(parser));

    return new Promise<void>((resolve, reject) => {
      loader.load(
        path,
        (gltf) => {
          const loadedVRM = gltf.userData.vrm as VRM;
          if (!loadedVRM) {
            reject(new Error("Failed to load VRM from GLTF"));
            return;
          }

          VRMUtils.removeUnnecessaryVertices(gltf.scene);
          VRMUtils.removeUnnecessaryJoints(gltf.scene);
          VRMUtils.rotateVRM0(loadedVRM);

          if (loadedVRM.lookAt) {
            loadedVRM.lookAt.target = undefined;
          }

          scene.add(loadedVRM.scene);
          vrm.value = loadedVRM;
          currentModelFormat.value = 'vrm';

          resolve();
        },
        (event) => {
          if (onProgress && event.total > 0) {
            onProgress(Math.round((event.loaded / event.total) * 100));
          }
        },
        (error) => {
          console.error("[use3DModel] VRM load failed:", error);
          reject(error);
        }
      );
    });
  }

  /** Load FBX model with auto-scale/center and optional AnimationMixer */
  function loadFBX(path: string, onProgress?: (pct: number) => void): Promise<void> {
    const loader = new FBXLoader();

    return new Promise<void>((resolve, reject) => {
      loader.load(
        path,
        (fbx) => {
          try {
            // Auto-scale & center (handles 0.01x, 1x, 100x FBX scales)
            autoScaleAndCenter(fbx, 1.7);

            // Setup AnimationMixer if FBX has embedded animations
            if (fbx.animations && fbx.animations.length > 0) {
              mixer = new THREE.AnimationMixer(fbx);
              const idleClip = fbx.animations[0];
              const action = mixer.clipAction(idleClip);
              action.play();
            }
            // If no animations, mixer stays null — safe, no crash

            scene.add(fbx);
            fbxModel = fbx;
            currentModelFormat.value = 'fbx';

            resolve();
          } catch (e: any) {
            console.error("[use3DModel] FBX post-process failed:", e);
            reject(e);
          }
        },
        (event) => {
          if (onProgress && event.total > 0) {
            onProgress(Math.round((event.loaded / event.total) * 100));
          }
        },
        (error) => {
          console.error("[use3DModel] FBX load failed:", error);
          reject(error);
        }
      );
    });
  }

  // ═══════════════════════════════════════════
  //  Render Loop (with procedural idle + adaptive throttle)
  // ═══════════════════════════════════════════
  let isWindowVisible = true;
  let lastFrameTime = 0;

  function startRenderLoop() {
    if (animFrameId !== null) return;

    // Adaptive throttle: reduce FPS when window hidden
    if (typeof document !== 'undefined') {
      document.addEventListener('visibilitychange', () => {
        isWindowVisible = !document.hidden;
      });
    }

    function animate(now: number) {
      animFrameId = requestAnimationFrame(animate);

      // Adaptive throttle: ~15fps when hidden (66ms interval)
      if (!isWindowVisible && now - lastFrameTime < 66) return;
      lastFrameTime = now;

      // ⚠ CRITICAL: Clamp delta to 1/30 (33ms) to prevent spring bone explosion
      // When FPS drops (throttle/background), large deltas cause physics integrator
      // to diverge → hair/clothes fly off. This is the Architect's fix.
      const rawDelta = clock.getDelta();
      const delta = Math.min(rawDelta, 1 / 30);

      if (vrm.value) {
        // Procedural idle animation (VRM only)
        updateIdle(delta);

        // Organic auto-blink (only when face tracking is OFF)
        if (!faceTrackingActive) {
          updateBlink(delta);
        }

        // Lip-sync
        if (lipSyncActive) {
          updateLipSync(delta);
        }

        // Spring-damped lookAt
        updateSpringLookAt(delta);

        // Micro-expressions
        updateMicroExpressions(delta);

        // VRM update (spring bones, etc.) — uses clamped delta!
        vrm.value.update(delta);
      }

      // FBX AnimationMixer update (runs independently of VRM)
      if (mixer) {
        mixer.update(delta);
      }

      if (renderer) {
        renderer.render(scene, camera);
      }
    }
    animate();
  }

  function stopRenderLoop() {
    if (animFrameId !== null) {
      cancelAnimationFrame(animFrameId);
      animFrameId = null;
    }
  }

  // ═══════════════════════════════════════════
  //  Procedural Idle — Breathing + Micro-Sway
  // ═══════════════════════════════════════════
  function updateIdle(delta: number) {
    if (!vrm.value) return;
    idleTime += delta;

    // 1. Breathing — subtle spine/chest oscillation
    const spine = vrm.value.humanoid?.getNormalizedBoneNode('spine');
    if (spine) {
      // Slow sine wave: 4-second cycle (15 breaths/min, natural resting rate)
      const breathCycle = Math.sin(idleTime * Math.PI * 0.5) * 0.008;
      spine.rotation.x = breathCycle;
    }

    // 2. OpenSimplex head micro-sway (natural, never repeats)
    if (!faceTrackingActive) {
      const head = vrm.value.humanoid?.getNormalizedBoneNode('head');
      if (head) {
        // 2D simplex noise at different time scales for organic motion
        const swayX = simplex2D(idleTime * 0.15, 0) * 0.005
                     + simplex2D(idleTime * 0.4, 1.7) * 0.002;
        const swayY = simplex2D(0, idleTime * 0.12) * 0.004
                     + simplex2D(2.3, idleTime * 0.35) * 0.002;
        head.rotation.x = swayX;
        head.rotation.y = swayY;
      }
    }
  }

  // ═══════════════════════════════════════════
  //  Organic Auto-Blink (easeOutQuad curve)
  // ═══════════════════════════════════════════
  function startAutoBlink() {
    // Reset blink state — the actual blinking runs inside render loop
    blinkTimer = 0;
    nextBlinkAt = randomBlinkInterval();
    blinkPhase = 'idle';
    isBlinking = false;
  }

  function updateBlink(delta: number) {
    if (!vrm.value?.expressionManager) return;
    const em = vrm.value.expressionManager;

    blinkTimer += delta;

    switch (blinkPhase) {
      case 'idle':
        if (blinkTimer >= nextBlinkAt) {
          blinkPhase = 'closing';
          blinkProgress = 0;
          isBlinking = true;
          // 20% chance of double-blink
          pendingDoubleBlink = Math.random() < 0.2;
        }
        break;

      case 'closing':
        // Close in ~60ms (easeOutQuad for natural speed)
        blinkProgress += delta / 0.06;
        if (blinkProgress >= 1) {
          blinkProgress = 1;
          blinkPhase = 'closed';
        }
        em.setValue('blink', easeOutQuad(blinkProgress));
        break;

      case 'closed':
        // Stay closed for 30-60ms (natural closed duration)
        blinkProgress += delta / (0.03 + Math.random() * 0.03);
        if (blinkProgress >= 2) {
          blinkPhase = 'opening';
          blinkProgress = 0;
        }
        em.setValue('blink', 1);
        break;

      case 'opening':
        // Open in ~100ms (slower than close — asymmetric = natural)
        blinkProgress += delta / 0.1;
        if (blinkProgress >= 1) {
          blinkProgress = 0;
          em.setValue('blink', 0);
          isBlinking = false;

          if (pendingDoubleBlink) {
            // Double-blink: blink again after tiny pause
            pendingDoubleBlink = false;
            blinkPhase = 'closing';
            blinkTimer = nextBlinkAt - 0.15; // Re-trigger in ~150ms
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

  // ═══════════════════════════════════════════
  //  Natural Lip-Sync (Multi-Vowel + Noise)
  // ═══════════════════════════════════════════
  function startLipSync() {
    if (lipSyncActive) return;
    lipSyncActive = true;
    lipTime = 0;
  }

  function updateLipSync(delta: number) {
    if (!vrm.value?.expressionManager || !lipSyncActive) return;
    const em = vrm.value.expressionManager;
    lipTime += delta;

    // Generate organic mouth movement via layered oscillation
    // This simulates natural speech patterns (NOT random rectangles)
    const speed = 8; // ~8 syllables/second

    // Primary jaw movement
    const jaw = Math.max(0,
      Math.sin(lipTime * speed) * 0.5
      + Math.sin(lipTime * speed * 1.7 + 0.5) * 0.25
      + Math.sin(lipTime * speed * 0.6 + 1.2) * 0.15
    );

    // Cycle through vowel shapes
    const vowelPhase = (lipTime * speed * 0.5) % 4;

    // 'aa' — open mouth (primary talking shape)
    em.setValue('aa', jaw * 0.8);

    // 'ih' — slight smile shape (secondary)
    em.setValue('ih', vowelPhase > 1 && vowelPhase < 2 ? jaw * 0.3 : 0);

    // 'ou' — rounded lips
    em.setValue('ou', vowelPhase > 2 && vowelPhase < 3 ? jaw * 0.4 : 0);

    // 'ee' — wide mouth
    em.setValue('ee', vowelPhase > 3 ? jaw * 0.3 : 0);
  }

  function stopLipSync() {
    lipSyncActive = false;
    lipTime = 0;
    if (!vrm.value?.expressionManager) return;
    const em = vrm.value.expressionManager;
    // Smooth close (don't snap to 0)
    em.setValue('aa', 0);
    em.setValue('ih', 0);
    em.setValue('ou', 0);
    em.setValue('ee', 0);
  }

  // ═══════════════════════════════════════════
  //  Micro-Expressions (Idle Personality)
  // ═══════════════════════════════════════════
  function updateMicroExpressions(delta: number) {
    if (!vrm.value?.expressionManager || faceTrackingActive || lipSyncActive) return;
    const em = vrm.value.expressionManager;

    microExprTimer += delta;

    if (!activeMicroExpr) {
      // Schedule next micro-expression
      if (microExprTimer >= nextMicroExprAt) {
        const options = ['happy', 'relaxed', 'surprised'];
        const weights = [0.5, 0.35, 0.15]; // Happy most common
        activeMicroExpr = weightedRandom(options, weights);
        microExprIntensity = 0;
        microExprFading = false;
        microExprTimer = 0;
      }
    } else {
      // Animate the micro-expression
      if (!microExprFading) {
        // Ramp up over ~400ms
        microExprIntensity += delta / 0.4;
        const targetIntensity = 0.2 + Math.random() * 0.3; // 0.2-0.5 (subtle)
        if (microExprIntensity >= targetIntensity) {
          microExprIntensity = targetIntensity;
          microExprFading = true;
          microExprTimer = 0;
        }
        em.setValue(activeMicroExpr, easeOutQuad(microExprIntensity));
      } else {
        // Hold for 0.5-1.5s then fade out
        if (microExprTimer < 0.5 + Math.random()) {
          em.setValue(activeMicroExpr, microExprIntensity);
        } else {
          // Fade out over ~600ms
          microExprIntensity -= delta / 0.6;
          if (microExprIntensity <= 0) {
            em.setValue(activeMicroExpr, 0);
            activeMicroExpr = null;
            microExprTimer = 0;
            nextMicroExprAt = 5 + Math.random() * 10; // 5-15s until next
          } else {
            em.setValue(activeMicroExpr, easeOutQuad(microExprIntensity));
          }
        }
      }
    }
  }

  // ═══════════════════════════════════════════
  //  Trigger Motion (Smooth Ramp, not Flash)
  // ═══════════════════════════════════════════
  function triggerMotion() {
    if (!vrm.value?.expressionManager) return;
    const em = vrm.value.expressionManager;

    // Pick expression with weighted randomness
    const options = ['happy', 'surprised', 'relaxed'];
    const weights = [0.45, 0.3, 0.25];
    const expr = weightedRandom(options, weights);
    const peakIntensity = 0.4 + Math.random() * 0.4; // 0.4-0.8

    // Smooth ramp up (300ms) → hold (200-500ms) → ramp down (500ms)
    const rampUpMs = 300;
    const holdMs = 200 + Math.random() * 300;
    const rampDownMs = 500;

    let elapsed = 0;
    const startTime = performance.now();

    function animateExpression() {
      elapsed = performance.now() - startTime;

      if (elapsed < rampUpMs) {
        // Ramp up with ease-out
        const t = elapsed / rampUpMs;
        em.setValue(expr, easeOutQuad(t) * peakIntensity);
      } else if (elapsed < rampUpMs + holdMs) {
        // Hold at peak with slight oscillation
        const holdT = (elapsed - rampUpMs) / holdMs;
        const wobble = Math.sin(holdT * Math.PI * 2) * 0.05;
        em.setValue(expr, peakIntensity + wobble);
      } else if (elapsed < rampUpMs + holdMs + rampDownMs) {
        // Ramp down with ease-in
        const t = (elapsed - rampUpMs - holdMs) / rampDownMs;
        em.setValue(expr, (1 - easeInQuad(t)) * peakIntensity);
      } else {
        // Done
        em.setValue(expr, 0);
        return;
      }

      requestAnimationFrame(animateExpression);
    }

    requestAnimationFrame(animateExpression);
  }

  // ═══════════════════════════════════════════
  //  Face Tracking — LookAt + Expressions
  // ═══════════════════════════════════════════

  /**
   * Drive VRM model eyes/head to follow user's face.
   * Uses spring-damped lerp for smooth, natural tracking.
   * @param yaw — horizontal angle in degrees (-45 to +45)
   * @param pitch — vertical angle in degrees (-35 to +35)
   */
  function updateLookAt(yaw: number, pitch: number) {
    // Set target — actual movement happens in updateSpringLookAt()
    targetYaw = yaw;
    targetPitch = pitch;
  }

  /**
   * Spring-damped LookAt update — called every frame.
   * Prevents robotic snap-to-target by exponentially decaying toward target.
   */
  function updateSpringLookAt(delta: number) {
    if (!vrm.value?.lookAt) return;

    // Exponential decay spring: 90% toward target per 100ms
    // This creates a smooth, natural "drag" feeling
    const springFactor = 1 - Math.pow(0.001, delta); // ~0.1-0.15 per frame at 60fps

    currentYaw = lerp(currentYaw, targetYaw, springFactor);
    currentPitch = lerp(currentPitch, targetPitch, springFactor);

    const la = vrm.value.lookAt;
    if (la.applier) {
      la.applier.applyYawPitch(currentYaw, currentPitch);
    }
  }

  /**
   * Map face tracking blendshapes → VRM expressions.
   * Provides real-time facial mirroring (user smiles → model smiles).
   */
  function updateExpressions(expressions: FaceExpressions) {
    if (!vrm.value?.expressionManager) return;
    const em = vrm.value.expressionManager;

    // Map face expressions → VRM expression names
    // Smooth factor prevents jittery transitions
    const smooth = 0.3;

    // Happy → VRM 'happy'
    if (expressions.happy > 0.15) {
      const current = em.getValue('happy') ?? 0;
      em.setValue('happy', lerp(current, expressions.happy, smooth));
    } else {
      em.setValue('happy', lerp(em.getValue('happy') ?? 0, 0, smooth));
    }

    // Surprised → VRM 'surprised'
    if (expressions.surprised > 0.2) {
      const current = em.getValue('surprised') ?? 0;
      em.setValue('surprised', lerp(current, expressions.surprised, smooth));
    } else {
      em.setValue('surprised', lerp(em.getValue('surprised') ?? 0, 0, smooth));
    }

    // Angry → VRM 'angry'
    if (expressions.angry > 0.2) {
      const current = em.getValue('angry') ?? 0;
      em.setValue('angry', lerp(current, expressions.angry, smooth));
    } else {
      em.setValue('angry', lerp(em.getValue('angry') ?? 0, 0, smooth));
    }

    // Blink — override auto-blink when face tracking is active
    if (faceTrackingActive) {
      em.setValue('blink', expressions.blink);
    }

    // Mouth open (for talking detection)
    if (expressions.mouthOpen > 0.1) {
      em.setValue('aa', expressions.mouthOpen * 0.8);
    } else {
      em.setValue('aa', lerp(em.getValue('aa') ?? 0, 0, smooth));
    }
  }

  /**
   * Toggle face tracking mode.
   * When active: disables auto-blink (real blinks take over).
   * When inactive: re-enables auto-blink.
   */
  function setFaceTrackingActive(active: boolean) {
    faceTrackingActive = active;
    if (active) {
      // Disable auto-blink — real blinks from camera
      blinkPhase = 'idle';
      isBlinking = false;
    } else {
      // Re-enable auto-blink
      startAutoBlink();
      // Smoothly reset face-driven expressions (don't snap to 0)
      if (vrm.value?.expressionManager) {
        const em = vrm.value.expressionManager;
        em.setValue('happy', 0);
        em.setValue('surprised', 0);
        em.setValue('angry', 0);
        em.setValue('aa', 0);
      }
      // Reset lookAt targets
      targetYaw = 0;
      targetPitch = 0;
    }
  }

  // ═══════════════════════════════════════════
  //  Dispose — Full VRAM Cleanup
  // ═══════════════════════════════════════════
  function dispose() {
    stopRenderLoop();
    stopLipSync();
    faceTrackingActive = false;
    activeMicroExpr = null;
    if (isBlinking) {
      isBlinking = false;
      blinkPhase = 'idle';
    }

    // Dispose all models (VRM + FBX)
    disposePreviousModel();

    if (renderer) {
      renderer.dispose();
      renderer.forceContextLoss();
      renderer = null;
    }
  }

  return {
    vrm,
    currentModelFormat,
    scene,
    camera,
    renderer,
    loadModel,
    initRenderer,
    startRenderLoop,
    stopRenderLoop,
    startAutoBlink,
    startLipSync,
    stopLipSync,
    triggerMotion,
    updateLookAt,
    updateExpressions,
    setFaceTrackingActive,
    dispose,
  };
}

/** Linear interpolation for smooth transitions */
function lerp(a: number, b: number, t: number): number {
  return a + (b - a) * t;
}

/** Ease-out quadratic — fast start, slow end (natural eyelid close) */
function easeOutQuad(t: number): number {
  return t * (2 - t);
}

/** Ease-in quadratic — slow start, fast end (natural expression fade) */
function easeInQuad(t: number): number {
  return t * t;
}

/** Random blink interval using Poisson-like distribution (2-6s base + jitter) */
function randomBlinkInterval(): number {
  // Average human blink rate: 15-20 blinks/min = every 3-4s
  // Add random jitter for natural variation
  return 2 + Math.random() * 4 + Math.random() * Math.random() * 3;
}

/** Weighted random selection */
function weightedRandom<T>(options: T[], weights: number[]): T {
  const total = weights.reduce((s, w) => s + w, 0);
  let r = Math.random() * total;
  for (let i = 0; i < options.length; i++) {
    r -= weights[i];
    if (r <= 0) return options[i];
  }
  return options[options.length - 1];
}
