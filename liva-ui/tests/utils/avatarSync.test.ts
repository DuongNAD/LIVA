import { describe, it, expect } from "vitest";
import {
  normalizeEngineMode,
  getActiveModelKey,
  isModelActive,
  buildAvatarConfigPatch,
  applyActiveFlags,
  type AvatarModelInfo,
} from "../../src/utils/avatarSync";

describe("avatarSync", () => {
  describe("normalizeEngineMode", () => {
    it("should return '2D' for '2d' and uppercase/lowercase variants", () => {
      expect(normalizeEngineMode("2d")).toBe("2D");
      expect(normalizeEngineMode("2D")).toBe("2D");
    });

    it("should return '3D' for '3d' and uppercase/lowercase variants", () => {
      expect(normalizeEngineMode("3d")).toBe("3D");
      expect(normalizeEngineMode("3D")).toBe("3D");
    });

    it("should return 'auto' for any other inputs, null, or undefined", () => {
      expect(normalizeEngineMode(null)).toBe("auto");
      expect(normalizeEngineMode(undefined)).toBe("auto");
      expect(normalizeEngineMode("unknown")).toBe("auto");
    });
  });

  describe("getActiveModelKey", () => {
    it("should return null if config is null or undefined", () => {
      expect(getActiveModelKey(null)).toBeNull();
      expect(getActiveModelKey(undefined)).toBeNull();
    });

    it("should resolve from ui.activeModel.filename with normalized path", () => {
      const config = {
        ui: {
          activeModel: {
            filename: "models\\vrm\\test.vrm",
          },
        },
      };
      expect(getActiveModelKey(config)).toBe("models/vrm/test.vrm");
    });

    it("should resolve from avatar.vrmModel if ui.activeModel.filename is missing", () => {
      const config = {
        avatar: {
          vrmModel: "models\\vrm\\test2.vrm",
        },
      };
      expect(getActiveModelKey(config)).toBe("models/vrm/test2.vrm");
    });

    it("should resolve from avatar.live2dModel if others are missing", () => {
      const config = {
        avatar: {
          live2dModel: "models\\live2d\\test3.json",
        },
      };
      expect(getActiveModelKey(config)).toBe("models/live2d/test3.json");
    });

    it("should resolve from avatar.activeModel if others are missing", () => {
      const config = {
        avatar: {
          activeModel: "models\\test4.vrm",
        },
      };
      expect(getActiveModelKey(config)).toBe("models/test4.vrm");
    });

    it("should return null if no active model fields are present", () => {
      expect(getActiveModelKey({})).toBeNull();
    });
  });

  describe("isModelActive", () => {
    const model3D: AvatarModelInfo = {
      name: "Test 3D",
      filename: "test.vrm",
      size: "10MB",
      isActive: false,
      type: "3d",
      format: "vrm",
    };

    it("should return false if active model key cannot be resolved", () => {
      expect(isModelActive(model3D, null)).toBe(false);
    });

    it("should return true if activeKey matches filename exactly", () => {
      const config = { avatar: { activeModel: "test.vrm" } };
      expect(isModelActive(model3D, config)).toBe(true);
    });

    it("should return true if activeKey matches models/vrm/filename", () => {
      const config = { avatar: { activeModel: "models/vrm/test.vrm" } };
      expect(isModelActive(model3D, config)).toBe(true);
    });

    it("should return true if activeKey matches models/live2d/filename", () => {
      const model2D: AvatarModelInfo = {
        name: "Test 2D",
        filename: "test.json",
        size: "5MB",
        isActive: false,
        type: "2d",
        format: "live2d",
      };
      const config = { avatar: { activeModel: "models/live2d/test.json" } };
      expect(isModelActive(model2D, config)).toBe(true);
    });

    it("should return false if activeKey does not match", () => {
      const config = { avatar: { activeModel: "other.vrm" } };
      expect(isModelActive(model3D, config)).toBe(false);
    });
  });

  describe("buildAvatarConfigPatch", () => {
    it("should construct patch for 3d model", () => {
      const model: AvatarModelInfo = {
        name: "Test 3D",
        filename: "test.vrm",
        size: "10MB",
        isActive: false,
        type: "3d",
        format: "vrm",
      };
      const patch = buildAvatarConfigPatch(model, "3D");
      expect(patch).toEqual({
        avatar: {
          engineMode: "3D",
          activeModel: "test.vrm",
          activeType: "3d",
          activeFormat: "vrm",
          vrmModel: "models/vrm/test.vrm",
        },
        ui: {
          avatarMode: "3D",
          activeModel: {
            filename: "models/vrm/test.vrm",
            type: "3d",
            format: "vrm",
          },
        },
      });
    });

    it("should construct patch for 2d model and fallback format", () => {
      const model: AvatarModelInfo = {
        name: "Test 2D",
        filename: "test.json",
        size: "5MB",
        isActive: false,
        type: "2d",
      };
      const patch = buildAvatarConfigPatch(model, "2D");
      expect(patch).toEqual({
        avatar: {
          engineMode: "2D",
          activeModel: "test.json",
          activeType: "2d",
          activeFormat: null,
          live2dModel: "models/live2d/test.json",
        },
        ui: {
          avatarMode: "2D",
          activeModel: {
            filename: "models/live2d/test.json",
            type: "2d",
            format: "live2d",
          },
        },
      });
    });
  });

  describe("applyActiveFlags", () => {
    it("should map models and set their isActive flag correctly", () => {
      const models: AvatarModelInfo[] = [
        { name: "M1", filename: "m1.vrm", size: "1MB", isActive: false, type: "3d" },
        { name: "M2", filename: "m2.vrm", size: "1MB", isActive: true, type: "3d" },
      ];
      const config = { avatar: { activeModel: "models/vrm/m1.vrm" } };
      const updated = applyActiveFlags(models, config);
      expect(updated[0].isActive).toBe(true);
      expect(updated[1].isActive).toBe(false);
    });
  });
});
