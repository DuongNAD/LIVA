import { describe, expect, it, vi } from "vitest";
import * as THREE from "three";

const { loadMock } = vi.hoisted(() => ({ loadMock: vi.fn() }));

vi.mock("three/examples/jsm/loaders/FBXLoader.js", () => ({
  FBXLoader: class {
    load = loadMock;
  },
}));

import {
  DEFAULT_MIXAMO_CLIP_PATHS,
  loadMixamoAnimationSet,
  loadMixamoClip,
} from "../../src/composables/mixamoClipLoader";

describe("loadMixamoClip", () => {
  it("loads the first FBX animation and retargets it without attaching the source rig", async () => {
    const sourceRoot = new THREE.Group();
    const sourceArm = new THREE.Object3D();
    sourceArm.name = "mixamorig:LeftArm";
    sourceRoot.add(sourceArm);
    sourceRoot.animations = [{
      name: "wave",
      duration: 1,
      tracks: [{
        name: "mixamorig:LeftArm.quaternion",
        times: [0],
        values: [0, 0, 0, 1],
      }],
    }];
    loadMock.mockImplementationOnce((_path, onLoad: (root: THREE.Group) => void) => onLoad(sourceRoot));

    const targetArm = new THREE.Object3D();
    const targetScene = new THREE.Group();
    targetScene.add(targetArm);
    const result = await loadMixamoClip("/animations/mixamo/wave.fbx", {
      scene: targetScene,
      humanoid: { getNormalizedBoneNode: (bone: string) => bone === "leftUpperArm" ? targetArm : null },
    } as never);

    expect(loadMock).toHaveBeenCalledWith(
      "/animations/mixamo/wave.fbx",
      expect.any(Function),
      undefined,
      expect.any(Function),
    );
    expect(Object.keys(result.tracks)).toEqual(["leftUpperArm"]);
    expect(targetScene.children).not.toContain(sourceRoot);
  });

  it("loads the standard animation set independently so missing clips keep the procedural fallback", async () => {
    const clip = { name: "idle", duration: 1, tracks: {} };
    const fakeLoader = vi.fn(async (path: string) => {
      if (path.endsWith("run.fbx")) throw new Error("missing run");
      return { ...clip, name: path };
    });

    const result = await loadMixamoAnimationSet({} as never, DEFAULT_MIXAMO_CLIP_PATHS, fakeLoader);

    expect(result.clips.idle?.name).toBe("/animations/mixamo/idle.fbx");
    expect(result.clips.run).toBeUndefined();
    expect(result.failures.run).toBe("missing run");
    expect(fakeLoader).toHaveBeenCalledTimes(6);
  });
});
