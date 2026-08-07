import { describe, expect, it } from "vitest";
import * as THREE from "three";
import { retargetMixamoClip } from "../../src/composables/mixamoClipLoader";

describe("retargetMixamoClip", () => {
  it("builds rest-pose bindings from the loaded Mixamo rig and normalized VRM bones", () => {
    const sourceRoot = new THREE.Group();
    const sourceArm = new THREE.Object3D();
    sourceArm.name = "mixamorig:LeftArm";
    sourceRoot.add(sourceArm);

    const targetArm = new THREE.Object3D();
    targetArm.name = "VRMNormalizedLeftUpperArm";
    const targetScene = new THREE.Group();
    targetScene.add(targetArm);

    const clip = retargetMixamoClip(sourceRoot, {
      name: "wave",
      duration: 1,
      tracks: [{
        name: "mixamorig:LeftArm.quaternion",
        times: [0, 1],
        values: [0, 0, 0, 1, Math.sin(0.5), 0, 0, Math.cos(0.5)],
      }],
    }, {
      scene: targetScene,
      humanoid: {
        getNormalizedBoneNode: (bone: string) => bone === "leftUpperArm" ? targetArm : null,
      },
    } as never);

    expect(Object.keys(clip.tracks)).toEqual(["leftUpperArm"]);
    expect(clip.tracks.leftUpperArm?.values).toHaveLength(8);
  });

  it("fails clearly when an FBX clip has no usable mapped quaternion tracks", () => {
    const sourceRoot = new THREE.Group();
    const targetScene = new THREE.Group();

    expect(() => retargetMixamoClip(sourceRoot, {
      name: "camera-only",
      duration: 1,
      tracks: [{ name: "Camera.position", times: [0], values: [0, 0, 0] }],
    }, {
      scene: targetScene,
      humanoid: { getNormalizedBoneNode: () => null },
    } as never)).toThrow("no mapped humanoid quaternion tracks");
  });
});
