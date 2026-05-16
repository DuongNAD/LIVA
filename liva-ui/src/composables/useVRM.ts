/**
 * useVRM.ts — Vue Composable for VRM 3D Avatar
 * ==============================================
 * Handles: model loading, auto-blink, lip-sync, lookAt, Deep Dispose (VRAM cleanup).
 * Face Tracking: updateLookAt() + updateExpressions() for MediaPipe integration.
 * Idle: OpenSimplex noise-based micro-sway + breathing.
 */
import * as THREE from "three";
import { GLTFLoader } from "three/examples/jsm/loaders/GLTFLoader.js";
import { VRMLoaderPlugin, VRM, VRMUtils } from "@pixiv/three-vrm";
import { shallowRef, type ShallowRef } from "vue";
import type { FaceExpressions } from "./useFaceTracking";
import { simplex2D } from "../utils/openSimplexNoise";
import { logger } from "../utils/logger";

export interface UseVRMReturn {
  vrm: ShallowRef<VRM | null>;
  scene: THREE.Scene;
  camera: THREE.PerspectiveCamera;
  renderer: THREE.WebGLRenderer | null;
  loadModel: (path: string) => Promise<void>;
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
 * Gọi khi swap model hoặc unmount component
 */
function deepDispose(scene: THREE.Object3D) {
  scene.traverse((object: any) => {
    if (!object.isMesh) return;

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
  });
}

export function useVRM(): UseVRMReturn {
  const vrm = shallowRef<VRM | null>(null);
  const scene = new THREE.Scene();
  const camera = new THREE.PerspectiveCamera(30, 500 / 700, 0.1, 20);
  let renderer: THREE.WebGLRenderer | null = null;
  let animFrameId: number | null = null;
  let clock = new THREE.Clock();

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
 // NOSONAR
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

    // Lighting — CRITICAL: VRM MToon shader cần đèn để không bị đen thui
    const hemiLight = new THREE.HemisphereLight(0xffffff, 0x444444, 0.6);
    scene.add(hemiLight);

    const dirLight = new THREE.DirectionalLight(0xffffff, 1);
    dirLight.position.set(1, 1, 1);
    scene.add(dirLight);

    // Subtle fill light from below
    const fillLight = new THREE.DirectionalLight(0x8888ff, 0.3);
    fillLight.position.set(-1, -0.5, 0.5);
    scene.add(fillLight);
  }

  // ═══════════════════════════════════════════
  //  Model Loading
  // ═══════════════════════════════════════════
  async function loadModel(path: string) {
    // Dispose previous model if exists
    if (vrm.value) {
      deepDispose(vrm.value.scene);
      scene.remove(vrm.value.scene);
      vrm.value = null;
    }

    const loader = new GLTFLoader();
    loader.register((parser: ConstructorParameters<typeof VRMLoaderPlugin>[0]) => new VRMLoaderPlugin(parser));

    return new Promise<void>((resolve, reject) => {
      loader.load(
        path,
        (gltf: { userData: { vrm?: VRM }; scene: THREE.Object3D }) => {
          const loadedVRM = gltf.userData.vrm as VRM;
          if (!loadedVRM) {
            reject(new Error("Failed to load VRM from GLTF"));
            return;
          }

          // Optimize: remove unnecessary bones/vertices
          VRMUtils.removeUnnecessaryVertices(gltf.scene);
          VRMUtils.removeUnnecessaryJoints(gltf.scene);

          // Rotate model to face camera (VRM models face +Z by default, camera is at +Z)
          VRMUtils.rotateVRM0(loadedVRM);

          // Disable default lookAt target → manual mode for face tracking
          if (loadedVRM.lookAt) {
            loadedVRM.lookAt.target = undefined;
          }

          scene.add(loadedVRM.scene);
          vrm.value = loadedVRM;

          resolve();
        },
        undefined,
        (error: unknown) => {
          logger.error('[useVRM]', 'Model load failed:', error instanceof Error ? error.message : String(error));
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
        // Procedural idle animation
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

      if (renderer) {
        renderer.render(scene, camera);
      }
    }
    animate(performance.now());
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
 // NOSONAR
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
 // NOSONAR
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
 // NOSONAR
        if (microExprIntensity >= targetIntensity) {
          microExprIntensity = targetIntensity;
          microExprFading = true;
          microExprTimer = 0;
        }
        em.setValue(activeMicroExpr, easeOutQuad(microExprIntensity));
      } else {
        // Hold for 0.5-1.5s then fade out
        if (microExprTimer < 0.5 + Math.random()) {
 // NOSONAR
          em.setValue(activeMicroExpr, microExprIntensity);
        } else {
          // Fade out over ~600ms
          microExprIntensity -= delta / 0.6;
          if (microExprIntensity <= 0) {
            em.setValue(activeMicroExpr, 0);
            activeMicroExpr = null;
            microExprTimer = 0;
            nextMicroExprAt = 5 + Math.random() * 10; // 5-15s until next
 // NOSONAR
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
 // NOSONAR

    // Smooth ramp up (300ms) → hold (200-500ms) → ramp down (500ms)
    const rampUpMs = 300;
    const holdMs = 200 + Math.random() * 300;
 // NOSONAR
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

    if (vrm.value) {
      deepDispose(vrm.value.scene);
      scene.remove(vrm.value.scene);
      vrm.value = null;
    }

    if (renderer) {
      renderer.dispose();
      renderer.forceContextLoss(); // Ép GPU xả sạch WebGL context
      renderer = null;
    }
  }

  return {
    vrm,
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
 // NOSONAR
}

/** Weighted random selection */
function weightedRandom<T>(options: T[], weights: number[]): T {
  const total = weights.reduce((s, w) => s + w, 0);
  let r = Math.random() * total;
 // NOSONAR
  for (let i = 0; i < options.length; i++) {
    r -= weights[i];
    if (r <= 0) return options[i];
  }
  return options[options.length - 1];
}
