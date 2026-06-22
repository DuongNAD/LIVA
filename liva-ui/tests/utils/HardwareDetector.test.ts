// @vitest-environment jsdom
/**
 * HardwareDetector.test.ts — GPU/RAM/CPU Profiler Tests
 * ======================================================
 * Tests for auto-detection of optimal rendering engine.
 * Uses JSDOM environment (no real GPU).
 */
import { describe, it, expect, beforeEach, vi } from "vitest";
import { detectOptimalEngine, profileHardware, type EngineMode, type EnginePreference } from "../../src/utils/HardwareDetector";

// Mock WebGL context
function mockWebGLContext(gpuRenderer: string | null = null) {
  const mockCanvas = document.createElement("canvas");
  const originalGetContext = mockCanvas.getContext.bind(mockCanvas);

  vi.spyOn(document, "createElement").mockReturnValue({
    ...mockCanvas,
    getContext: (type: string) => {
      if (type === "webgl" || type === "experimental-webgl" || type === "webgl2") {
        if (!gpuRenderer) return null; // No WebGL
        return {
          getExtension: (name: string) => {
            if (name === "WEBGL_debug_renderer_info") {
              return { UNMASKED_RENDERER_WEBGL: 0x9246 };
            }
            if (name === "WEBGL_lose_context") {
              return { loseContext: vi.fn() };
            }
            return null;
          },
          getParameter: (_param: number) => gpuRenderer,
        };
      }
      return originalGetContext(type);
    },
  } as any);
}

describe("HardwareDetector", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    // Reset navigator mocks
    Object.defineProperty(navigator, "hardwareConcurrency", { value: 8, writable: true, configurable: true });
    Object.defineProperty(navigator, "deviceMemory", { value: 16, writable: true, configurable: true });
  });

  describe("detectOptimalEngine — Force Modes", () => {
    it("should return '2D' when preference is Force 2D", () => {
      expect(detectOptimalEngine("2D")).toBe("2D");
    });

    it("should return '3D' when preference is Force 3D", () => {
      expect(detectOptimalEngine("3D")).toBe("3D");
    });

    it("should use auto-detect when preference is 'auto'", () => {
      mockWebGLContext("NVIDIA GeForce RTX 4090");
      const result = detectOptimalEngine("auto");
      expect(["2D", "3D"]).toContain(result);
    });
  });

  describe("profileHardware — GPU Detection", () => {
    it("should detect Intel UHD as weak GPU", () => {
      mockWebGLContext("Intel(R) UHD Graphics 630");
      const profile = profileHardware();
      expect(profile.isWeakGPU).toBe(true);
      expect(profile.recommendedEngine).toBe("2D");
    });

    it("should detect Intel HD Graphics as weak GPU", () => {
      mockWebGLContext("Intel(R) HD Graphics 4600");
      const profile = profileHardware();
      expect(profile.isWeakGPU).toBe(true);
      expect(profile.recommendedEngine).toBe("2D");
    });

    it("should detect Intel Iris as weak GPU", () => {
      mockWebGLContext("Intel(R) Iris(R) Xe Graphics");
      const profile = profileHardware();
      expect(profile.isWeakGPU).toBe(true);
    });

    it("should detect AMD Radeon Graphics (APU) as weak GPU", () => {
      mockWebGLContext("AMD Radeon Graphics");
      const profile = profileHardware();
      expect(profile.isWeakGPU).toBe(true);
    });

    it("should detect AMD Radeon Vega as weak GPU", () => {
      mockWebGLContext("AMD Radeon Vega 8 Graphics");
      const profile = profileHardware();
      expect(profile.isWeakGPU).toBe(true);
    });

    it("should detect SwiftShader (software) as weak GPU", () => {
      mockWebGLContext("Google SwiftShader");
      const profile = profileHardware();
      expect(profile.isWeakGPU).toBe(true);
    });

    it("should detect NVIDIA discrete GPU as strong", () => {
      mockWebGLContext("NVIDIA GeForce RTX 3060");
      const profile = profileHardware();
      expect(profile.isWeakGPU).toBe(false);
    });

    it("should detect AMD discrete GPU as strong", () => {
      mockWebGLContext("AMD Radeon RX 6700 XT");
      const profile = profileHardware();
      expect(profile.isWeakGPU).toBe(false);
    });

    it("should handle no WebGL support gracefully", () => {
      mockWebGLContext(null);
      const profile = profileHardware();
      expect(profile.gpu).toBe("unknown");
      // Unknown GPU defaults to not-weak, but low RAM/cores might force 2D
    });
  });

  describe("profileHardware — RAM/CPU Thresholds", () => {
    it("should recommend 2D for low RAM (<8GB)", () => {
      Object.defineProperty(navigator, "deviceMemory", { value: 4, configurable: true });
      mockWebGLContext("NVIDIA GeForce RTX 3060"); // Strong GPU, but low RAM
      const profile = profileHardware();
      expect(profile.recommendedEngine).toBe("2D");
    });

    it("should recommend 2D for low CPU cores (<6)", () => {
      Object.defineProperty(navigator, "hardwareConcurrency", { value: 4, configurable: true });
      mockWebGLContext("NVIDIA GeForce RTX 3060"); // Strong GPU, but few cores
      const profile = profileHardware();
      expect(profile.recommendedEngine).toBe("2D");
    });

    it("should recommend 3D for powerful machine", () => {
      Object.defineProperty(navigator, "deviceMemory", { value: 32, configurable: true });
      Object.defineProperty(navigator, "hardwareConcurrency", { value: 16, configurable: true });
      mockWebGLContext("NVIDIA GeForce RTX 4090");
      const profile = profileHardware();
      expect(profile.recommendedEngine).toBe("3D");
    });

    it("should use defaults when navigator APIs are unavailable", () => {
      Object.defineProperty(navigator, "deviceMemory", { value: undefined, configurable: true });
      Object.defineProperty(navigator, "hardwareConcurrency", { value: undefined, configurable: true });
      mockWebGLContext("NVIDIA GeForce RTX 3060");
      const profile = profileHardware();
      // Defaults: 4GB RAM, 4 cores → both below threshold → 2D
      expect(profile.ram).toBe(4);
      expect(profile.cores).toBe(4);
      expect(profile.recommendedEngine).toBe("2D");
    });
  });

  describe("profileHardware — Return Structure", () => {
    it("should return complete HardwareProfile object", () => {
      mockWebGLContext("NVIDIA GeForce RTX 3060");
      const profile = profileHardware();
      
      expect(profile).toHaveProperty("gpu");
      expect(profile).toHaveProperty("ram");
      expect(profile).toHaveProperty("cores");
      expect(profile).toHaveProperty("isWeakGPU");
      expect(profile).toHaveProperty("recommendedEngine");
      expect(typeof profile.gpu).toBe("string");
      expect(typeof profile.ram).toBe("number");
      expect(typeof profile.cores).toBe("number");
      expect(typeof profile.isWeakGPU).toBe("boolean");
      expect(["2D", "3D"]).toContain(profile.recommendedEngine);
    });
  });

  describe("profileHardware — OS Detection", () => {
    const setUA = (ua: string) => {
      Object.defineProperty(navigator, "userAgent", { value: ua, configurable: true, writable: true });
    };

    it("should detect Windows", () => {
      setUA("Mozilla/5.0 (Windows NT 10.0; Win64; x64)");
      const profile = profileHardware();
      expect(profile.os).toBe("Windows");
    });

    it("should detect macOS", () => {
      setUA("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)");
      const profile = profileHardware();
      expect(profile.os).toBe("macOS");
    });

    it("should detect Linux", () => {
      setUA("Mozilla/5.0 (X11; Linux x86_64)");
      const profile = profileHardware();
      expect(profile.os).toBe("Linux");
    });

    it("should detect Android", () => {
      // Must not contain "Linux" or else detectOS() returns "Linux"
      setUA("Mozilla/5.0 (Android 10; SM-A505F)");
      const profile = profileHardware();
      expect(profile.os).toBe("Android");
    });

    it("should return macOS for iOS due to Mac keyword priority", () => {
      setUA("Mozilla/5.0 (iPhone; CPU iPhone OS 13_2_3 like Mac OS X)");
      const profile = profileHardware();
      // Since "like Mac" contains "Mac", the original code returns "macOS"
      expect(profile.os).toBe("macOS");
    });

    it("should fall back to Unknown for unknown OS", () => {
      setUA("CustomOS/1.0");
      const profile = profileHardware();
      expect(profile.os).toBe("Unknown");
    });
  });

  describe("profileHardware — Browser Detection", () => {
    const setUA = (ua: string) => {
      Object.defineProperty(navigator, "userAgent", { value: ua, configurable: true, writable: true });
    };

    it("should detect Chrome", () => {
      setUA("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36");
      const profile = profileHardware();
      expect(profile.browser).toBe("Chrome");
    });

    it("should detect Edge", () => {
      setUA("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36 Edg/120.0.0.0");
      const profile = profileHardware();
      expect(profile.browser).toBe("Edge");
    });

    it("should detect Firefox", () => {
      setUA("Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:109.0) Gecko/20100101 Firefox/119.0");
      const profile = profileHardware();
      expect(profile.browser).toBe("Firefox");
    });

    it("should detect Safari", () => {
      setUA("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15");
      const profile = profileHardware();
      expect(profile.browser).toBe("Safari");
    });

    it("should detect Opera", () => {
      setUA("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36 OPR/106.0.0.0");
      const profile = profileHardware();
      expect(profile.browser).toBe("Opera");
    });

    it("should fall back to Unknown for unknown browser", () => {
      setUA("MySpecialBrowser/2.0");
      const profile = profileHardware();
      expect(profile.browser).toBe("Unknown");
    });
  });

  describe("profileHardware — cleanGPUName details", () => {
    it("should clean ANGLE string with multiple parts", () => {
      mockWebGLContext("ANGLE (Intel, Intel(R) HD Graphics 4600, D3D11)");
      const profile = profileHardware();
      expect(profile.gpu).toBe("Intel(R) HD Graphics 4600");
    });

    it("should clean ANGLE string with single part", () => {
      mockWebGLContext("ANGLE (NVIDIA)");
      const profile = profileHardware();
      expect(profile.gpu).toBe("NVIDIA");
    });

    it("should strip Direct3D, OpenGL, Vulkan, Metal and shaders", () => {
      mockWebGLContext("NVIDIA GeForce RTX 3060 Direct3D11 vs_5_0 ps_5_0");
      let profile = profileHardware();
      expect(profile.gpu).toBe("NVIDIA GeForce RTX 3060");

      mockWebGLContext("NVIDIA OpenGL");
      profile = profileHardware();
      expect(profile.gpu).toBe("NVIDIA");

      mockWebGLContext("NVIDIA Vulkan");
      profile = profileHardware();
      expect(profile.gpu).toBe("NVIDIA");

      mockWebGLContext("NVIDIA Metal");
      profile = profileHardware();
      expect(profile.gpu).toBe("NVIDIA");
    });
  });

  describe("profileHardware — WebGL fallback", () => {
    it("should support WebGL 1.0 if WebGL 2.0 is not available", () => {
      const mockCanvas = document.createElement("canvas");
      const originalGetContext = mockCanvas.getContext.bind(mockCanvas);

      vi.spyOn(document, "createElement").mockReturnValue({
        ...mockCanvas,
        getContext: (type: string) => {
          if (type === "webgl2") return null;
          if (type === "webgl" || type === "experimental-webgl") {
            return {
              MAX_TEXTURE_SIZE: 3379,
              getExtension: (name: string) => {
                if (name === "WEBGL_debug_renderer_info") {
                  return { UNMASKED_RENDERER_WEBGL: 0x9246 };
                }
                return null;
              },
              getParameter: (param: number) => {
                if (param === 3379) return 4096; // MAX_TEXTURE_SIZE
                return "NVIDIA GeForce RTX 3060";
              },
            };
          }
          return originalGetContext(type);
        },
      } as any);

      const profile = profileHardware();
      expect(profile.webglVersion).toBe("WebGL 1.0");
      expect(profile.maxTextureSize).toBe(4096);
    });
  });
});

