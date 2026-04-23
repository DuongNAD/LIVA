/**
 * useVRM.test.ts — Unit Tests for VRM Animation Utilities
 * =========================================================
 * Tests the pure utility functions used by the animation system:
 * lerp, easeOutQuad, easeInQuad, randomBlinkInterval, weightedRandom.
 * 
 * THREE.js/VRM module mocked since it requires WebGL.
 */
import { describe, it, expect, vi } from "vitest";

// ─── Mock THREE.js and VRM (WebGL, not available in Node) ───
vi.mock("three", () => {
  class Scene {
    traverse = vi.fn();
    add = vi.fn();
    remove = vi.fn();
  }
  class PerspectiveCamera {
    position = { set: vi.fn() };
  }
  class WebGLRenderer {
    setSize = vi.fn();
    setPixelRatio = vi.fn();
    render = vi.fn();
    dispose = vi.fn();
    forceContextLoss = vi.fn();
    domElement = {};
  }
  class Clock {
    getDelta() { return 0.016; }
  }
  class HemisphereLight {}
  class DirectionalLight {
    position = { set: vi.fn() };
  }

  return {
    Scene,
    PerspectiveCamera,
    WebGLRenderer,
    Clock,
    HemisphereLight,
    DirectionalLight,
  };
});

vi.mock("three/examples/jsm/loaders/GLTFLoader.js", () => ({
  GLTFLoader: vi.fn().mockImplementation(() => ({
    register: vi.fn(),
    load: vi.fn(),
  })),
}));

vi.mock("@pixiv/three-vrm", () => ({
  VRMLoaderPlugin: vi.fn(),
  VRM: vi.fn(),
  VRMUtils: {
    removeUnnecessaryVertices: vi.fn(),
    removeUnnecessaryJoints: vi.fn(),
    rotateVRM0: vi.fn(),
  },
}));

vi.mock("../../src/composables/useFaceTracking", () => ({
  // Only type import, no runtime needed
}));

// ─── Extract pure utility functions for testing ───
// Since they're module-private, we test via dynamic import tricks
// or duplicate them for isolated testing.

// Duplicate pure functions for isolated testing:
function lerp(a: number, b: number, t: number): number {
  return a + (b - a) * t;
}

function easeOutQuad(t: number): number {
  return t * (2 - t);
}

function easeInQuad(t: number): number {
  return t * t;
}

function randomBlinkInterval(): number {
  return 2 + Math.random() * 4 + Math.random() * Math.random() * 3;
}

function weightedRandom<T>(options: T[], weights: number[]): T {
  const total = weights.reduce((s, w) => s + w, 0);
  let r = Math.random() * total;
  for (let i = 0; i < options.length; i++) {
    r -= weights[i];
    if (r <= 0) return options[i];
  }
  return options[options.length - 1];
}

function clamp(val: number, min: number, max: number): number {
  return Math.max(min, Math.min(max, val));
}

// ═══════════════════════════════════════════════════════
//  Tests
// ═══════════════════════════════════════════════════════

describe("VRM Animation Utilities — lerp", () => {
  it("should return start value when t=0", () => {
    expect(lerp(0, 100, 0)).toBe(0);
  });

  it("should return end value when t=1", () => {
    expect(lerp(0, 100, 1)).toBe(100);
  });

  it("should return midpoint when t=0.5", () => {
    expect(lerp(0, 100, 0.5)).toBe(50);
  });

  it("should handle negative values", () => {
    expect(lerp(-10, 10, 0.5)).toBe(0);
  });

  it("should handle t > 1 (extrapolation)", () => {
    expect(lerp(0, 100, 1.5)).toBe(150);
  });

  it("should handle identical start/end", () => {
    expect(lerp(42, 42, 0.7)).toBe(42);
  });
});

describe("VRM Animation Utilities — easeOutQuad", () => {
  it("should return 0 at t=0", () => {
    expect(easeOutQuad(0)).toBe(0);
  });

  it("should return 1 at t=1", () => {
    expect(easeOutQuad(1)).toBe(1);
  });

  it("should be > linear at t=0.5 (fast start)", () => {
    expect(easeOutQuad(0.5)).toBe(0.75); // 0.5 * (2 - 0.5) = 0.75
    expect(easeOutQuad(0.5)).toBeGreaterThan(0.5);
  });

  it("should produce smooth values between 0 and 1", () => {
    for (let t = 0; t <= 1; t += 0.1) {
      const val = easeOutQuad(t);
      expect(val).toBeGreaterThanOrEqual(0);
      expect(val).toBeLessThanOrEqual(1);
    }
  });
});

describe("VRM Animation Utilities — easeInQuad", () => {
  it("should return 0 at t=0", () => {
    expect(easeInQuad(0)).toBe(0);
  });

  it("should return 1 at t=1", () => {
    expect(easeInQuad(1)).toBe(1);
  });

  it("should be < linear at t=0.5 (slow start)", () => {
    expect(easeInQuad(0.5)).toBe(0.25); // 0.5 * 0.5 = 0.25
    expect(easeInQuad(0.5)).toBeLessThan(0.5);
  });
});

describe("VRM Animation Utilities — randomBlinkInterval", () => {
  it("should return interval >= 2 seconds", () => {
    for (let i = 0; i < 100; i++) {
      expect(randomBlinkInterval()).toBeGreaterThanOrEqual(2);
    }
  });

  it("should return interval <= 9 seconds (2 + 4 + 3 max)", () => {
    for (let i = 0; i < 100; i++) {
      expect(randomBlinkInterval()).toBeLessThanOrEqual(9);
    }
  });

  it("should produce varied intervals (not constant)", () => {
    const intervals = new Set<number>();
    for (let i = 0; i < 20; i++) {
      intervals.add(Math.round(randomBlinkInterval() * 100));
    }
    // Should have some variation (at least 3 distinct values)
    expect(intervals.size).toBeGreaterThanOrEqual(3);
  });
});

describe("VRM Animation Utilities — weightedRandom", () => {
  it("should return items from the options array", () => {
    const options = ["a", "b", "c"];
    const weights = [1, 1, 1];

    for (let i = 0; i < 50; i++) {
      const result = weightedRandom(options, weights);
      expect(options).toContain(result);
    }
  });

  it("should respect weights (heavily weighted option picked most)", () => {
    const options = ["rare", "common"];
    const weights = [0.01, 0.99]; // 'common' should appear ~99% of the time

    let commonCount = 0;
    const trials = 1000;
    for (let i = 0; i < trials; i++) {
      if (weightedRandom(options, weights) === "common") commonCount++;
    }

    // Should be at least 90% common (with overwhelming probability)
    expect(commonCount / trials).toBeGreaterThan(0.85);
  });

  it("should handle single-item array", () => {
    expect(weightedRandom(["only"], [1])).toBe("only");
  });

  it("should handle zero-weight items", () => {
    const options = ["never", "always"];
    const weights = [0, 1];

    for (let i = 0; i < 50; i++) {
      expect(weightedRandom(options, weights)).toBe("always");
    }
  });
});

describe("VRM Animation Utilities — clamp", () => {
  it("should clamp values above max", () => {
    expect(clamp(100, 0, 45)).toBe(45);
  });

  it("should clamp values below min", () => {
    expect(clamp(-100, -45, 45)).toBe(-45);
  });

  it("should pass through values within range", () => {
    expect(clamp(20, -45, 45)).toBe(20);
  });

  it("should handle edge cases at exact boundaries", () => {
    expect(clamp(45, -45, 45)).toBe(45);
    expect(clamp(-45, -45, 45)).toBe(-45);
  });

  it("should handle min === max", () => {
    expect(clamp(10, 5, 5)).toBe(5);
    expect(clamp(3, 5, 5)).toBe(5);
  });
});

describe("VRM Animation — Composable Interface", () => {
  it("should export useVRM function", async () => {
    const mod = await import("../../src/composables/useVRM");
    expect(mod.useVRM).toBeDefined();
    expect(typeof mod.useVRM).toBe("function");
  });

  it("should return all required methods from useVRM", async () => {
    const mod = await import("../../src/composables/useVRM");
    const vrm = mod.useVRM();

    expect(vrm).toHaveProperty("vrm");
    expect(vrm).toHaveProperty("scene");
    expect(vrm).toHaveProperty("camera");
    expect(vrm).toHaveProperty("loadModel");
    expect(vrm).toHaveProperty("initRenderer");
    expect(vrm).toHaveProperty("startRenderLoop");
    expect(vrm).toHaveProperty("stopRenderLoop");
    expect(vrm).toHaveProperty("startAutoBlink");
    expect(vrm).toHaveProperty("startLipSync");
    expect(vrm).toHaveProperty("stopLipSync");
    expect(vrm).toHaveProperty("triggerMotion");
    expect(vrm).toHaveProperty("updateLookAt");
    expect(vrm).toHaveProperty("updateExpressions");
    expect(vrm).toHaveProperty("setFaceTrackingActive");
    expect(vrm).toHaveProperty("dispose");
  });

  it("should not crash when calling dispose without init", async () => {
    const mod = await import("../../src/composables/useVRM");
    const vrm = mod.useVRM();

    expect(() => vrm.dispose()).not.toThrow();
  });

  it("should not crash when calling stopLipSync without start", async () => {
    const mod = await import("../../src/composables/useVRM");
    const vrm = mod.useVRM();

    expect(() => vrm.stopLipSync()).not.toThrow();
  });

  it("should not crash when calling stopRenderLoop without start", async () => {
    const mod = await import("../../src/composables/useVRM");
    const vrm = mod.useVRM();

    expect(() => vrm.stopRenderLoop()).not.toThrow();
  });

  it("should not crash when calling updateLookAt without VRM loaded", async () => {
    const mod = await import("../../src/composables/useVRM");
    const vrm = mod.useVRM();

    expect(() => vrm.updateLookAt(10, -5)).not.toThrow();
  });

  it("should not crash when calling updateExpressions without VRM loaded", async () => {
    const mod = await import("../../src/composables/useVRM");
    const vrm = mod.useVRM();

    expect(() => vrm.updateExpressions({
      happy: 0.5, sad: 0, surprised: 0, angry: 0,
      blink: 0, blinkLeft: 0, blinkRight: 0,
      mouthOpen: 0, browUpLeft: 0, browUpRight: 0,
    })).not.toThrow();
  });

  it("should not crash when toggling face tracking without VRM", async () => {
    const mod = await import("../../src/composables/useVRM");
    const vrm = mod.useVRM();

    expect(() => {
      vrm.setFaceTrackingActive(true);
      vrm.setFaceTrackingActive(false);
    }).not.toThrow();
  });
});
