import * as THREE from "three";
import { FBXLoader } from "three/examples/jsm/loaders/FBXLoader.js";
import type { VRM } from "@pixiv/three-vrm";
import {
  MixamoRetargeter,
  mixamoBoneToVrm,
  type QuaternionTuple,
  type RetargetBinding,
  type RetargetedClip,
  type SourceAnimationClip,
} from "./mixamoRetarget";
import type { AvatarClipState, ControlledBone } from "./useAvatarAnimation";

export const DEFAULT_MIXAMO_CLIP_PATHS: Record<AvatarClipState, string> = {
  idle: "/animations/mixamo/idle.fbx",
  walk: "/animations/mixamo/walk.fbx",
  run: "/animations/mixamo/run.fbx",
  jump: "/animations/mixamo/jump.fbx",
  wave: "/animations/mixamo/wave.fbx",
  thinking: "/animations/mixamo/thinking.fbx",
};

const toTuple = (value: THREE.Quaternion): QuaternionTuple => [value.x, value.y, value.z, value.w];

export function retargetMixamoClip(
  sourceRoot: THREE.Object3D,
  sourceClip: SourceAnimationClip,
  targetVrm: VRM,
): RetargetedClip {
  sourceRoot.updateMatrixWorld(true);
  targetVrm.scene.updateMatrixWorld(true);

  const sourceNodes = new Map<ControlledBone, THREE.Object3D>();
  sourceRoot.traverse((node) => {
    const bone = mixamoBoneToVrm(node.name);
    if (bone && !sourceNodes.has(bone)) sourceNodes.set(bone, node);
  });

  const bindings: RetargetBinding[] = [];
  for (const [bone, sourceNode] of sourceNodes) {
    const targetNode = targetVrm.humanoid.getNormalizedBoneNode(bone);
    if (!targetNode) continue;
    const sourceWorld = sourceNode.getWorldQuaternion(new THREE.Quaternion());
    const targetWorld = targetNode.getWorldQuaternion(new THREE.Quaternion());
    bindings.push({
      bone,
      sourceRestLocal: toTuple(sourceNode.quaternion),
      sourceRestWorld: toTuple(sourceWorld),
      targetRestLocal: toTuple(targetNode.quaternion),
      targetRestWorld: toTuple(targetWorld),
    });
  }

  const clip = new MixamoRetargeter(bindings).retargetClip(sourceClip);
  if (Object.keys(clip.tracks).length === 0) {
    throw new Error(`Mixamo clip "${sourceClip.name}" has no mapped humanoid quaternion tracks`);
  }
  return clip;
}

export function loadMixamoClip(path: string, targetVrm: VRM): Promise<RetargetedClip> {
  const loader = new FBXLoader();
  return new Promise((resolve, reject) => {
    loader.load(
      path,
      (sourceRoot) => {
        const sourceClip = sourceRoot.animations[0] as SourceAnimationClip | undefined;
        if (!sourceClip) {
          reject(new Error(`Mixamo FBX "${path}" does not contain an animation clip`));
          return;
        }
        try {
          resolve(retargetMixamoClip(sourceRoot, sourceClip, targetVrm));
        } catch (error) {
          reject(error);
        }
      },
      undefined,
      reject,
    );
  });
}

export async function loadMixamoAnimationSet(
  targetVrm: VRM,
  paths: Record<AvatarClipState, string> = DEFAULT_MIXAMO_CLIP_PATHS,
  loader: (path: string, vrm: VRM) => Promise<RetargetedClip> = loadMixamoClip,
): Promise<{
  clips: Partial<Record<AvatarClipState, RetargetedClip>>;
  failures: Partial<Record<AvatarClipState, string>>;
}> {
  const clips: Partial<Record<AvatarClipState, RetargetedClip>> = {};
  const failures: Partial<Record<AvatarClipState, string>> = {};
  const entries = Object.entries(paths) as [AvatarClipState, string][];

  await Promise.all(entries.map(async ([state, path]) => {
    try {
      clips[state] = await loader(path, targetVrm);
    } catch (error) {
      failures[state] = error instanceof Error ? error.message : String(error);
    }
  }));

  return { clips, failures };
}
