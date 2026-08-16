/** Các xương mà lớp animation điều khiển. Cố tình không có spine/head/neck. */
export const CONTROLLED_BONES = [
  "leftUpperLeg", "leftLowerLeg", "leftFoot",
  "rightUpperLeg", "rightLowerLeg", "rightFoot",
  "leftUpperArm", "leftLowerArm",
  "rightUpperArm", "rightLowerArm",
  "hips",
] as const;

export type ControlledBone = (typeof CONTROLLED_BONES)[number];

/** Góc Euler (radian) theo thứ tự X, Y, Z */
export type Euler3 = readonly [number, number, number];
export type Pose = Partial<Record<ControlledBone, Euler3>>;

export type QuaternionTuple = readonly [number, number, number, number];

export interface RetargetBinding {
  bone: ControlledBone;
  sourceRestLocal: QuaternionTuple;
  sourceRestWorld: QuaternionTuple;
  targetRestLocal: QuaternionTuple;
  targetRestWorld: QuaternionTuple;
}

export interface SourceKeyframeTrack {
  name: string;
  times: ArrayLike<number>;
  values: ArrayLike<number>;
}

export interface SourceAnimationClip {
  name: string;
  duration: number;
  tracks: SourceKeyframeTrack[];
}

export interface RetargetedTrack {
  times: number[];
  values: number[];
}

export interface RetargetedClip {
  name: string;
  duration: number;
  tracks: Partial<Record<ControlledBone, RetargetedTrack>>;
}

const MIXAMO_TO_VRM: Record<string, ControlledBone> = {
  hips: "hips",
  leftupleg: "leftUpperLeg",
  leftleg: "leftLowerLeg",
  leftfoot: "leftFoot",
  rightupleg: "rightUpperLeg",
  rightleg: "rightLowerLeg",
  rightfoot: "rightFoot",
  leftarm: "leftUpperArm",
  leftforearm: "leftLowerArm",
  rightarm: "rightUpperArm",
  rightforearm: "rightLowerArm",
};

export function mixamoBoneToVrm(name: string): ControlledBone | null {
  const leaf = name.split(/[|/]/).at(-1) ?? name;
  const normalized = leaf.toLowerCase().replace(/^mixamorig:?/, "").replace(/[^a-z0-9]/g, "");
  return MIXAMO_TO_VRM[normalized] ?? null;
}

function normalizeQuaternion(value: QuaternionTuple): QuaternionTuple {
  const length = Math.hypot(value[0], value[1], value[2], value[3]);
  if (length <= Number.EPSILON) return [0, 0, 0, 1];
  return [value[0] / length, value[1] / length, value[2] / length, value[3] / length];
}

function multiplyQuaternion(a: QuaternionTuple, b: QuaternionTuple): QuaternionTuple {
  return normalizeQuaternion([
    a[3] * b[0] + a[0] * b[3] + a[1] * b[2] - a[2] * b[1],
    a[3] * b[1] - a[0] * b[2] + a[1] * b[3] + a[2] * b[0],
    a[3] * b[2] + a[0] * b[1] - a[1] * b[0] + a[2] * b[3],
    a[3] * b[3] - a[0] * b[0] - a[1] * b[1] - a[2] * b[2],
  ]);
}

function invertQuaternion(value: QuaternionTuple): QuaternionTuple {
  const normalized = normalizeQuaternion(value);
  return [-normalized[0], -normalized[1], -normalized[2], normalized[3]];
}

function tupleAt(values: ArrayLike<number>, index: number): QuaternionTuple {
  return [values[index], values[index + 1], values[index + 2], values[index + 3]];
}

function slerpQuaternion(a: QuaternionTuple, b: QuaternionTuple, weight: number): QuaternionTuple {
  let end: QuaternionTuple = b;
  let dot = a[0] * b[0] + a[1] * b[1] + a[2] * b[2] + a[3] * b[3];
  if (dot < 0) {
    end = [-b[0], -b[1], -b[2], -b[3]];
    dot = -dot;
  }
  if (dot > 0.9995) {
    return normalizeQuaternion([
      a[0] + (end[0] - a[0]) * weight,
      a[1] + (end[1] - a[1]) * weight,
      a[2] + (end[2] - a[2]) * weight,
      a[3] + (end[3] - a[3]) * weight,
    ]);
  }

  const angle = Math.acos(Math.min(Math.max(dot, -1), 1));
  const denominator = Math.sin(angle);
  const fromWeight = Math.sin((1 - weight) * angle) / denominator;
  const toWeight = Math.sin(weight * angle) / denominator;
  return normalizeQuaternion([
    a[0] * fromWeight + end[0] * toWeight,
    a[1] * fromWeight + end[1] * toWeight,
    a[2] * fromWeight + end[2] * toWeight,
    a[3] * fromWeight + end[3] * toWeight,
  ]);
}

function quaternionToEuler(value: QuaternionTuple): readonly [number, number, number] {
  const [x, y, z, w] = normalizeQuaternion(value);
  const pitch = Math.asin(Math.min(Math.max(2 * (x * z + w * y), -1), 1));
  return [
    Math.atan2(2 * (w * x - y * z), 1 - 2 * (x * x + y * y)),
    pitch,
    Math.atan2(2 * (w * z - x * y), 1 - 2 * (y * y + z * z)),
  ];
}

/**
 * Chỉ số nhỏ nhất có `times[i] >= time`, hoặc `times.length` nếu không có.
 *
 * U31(c): thay `Array.prototype.findIndex`, vốn quét tuyến tính **từ đầu, cho
 * mỗi xương, mỗi khung hình**. Chọn tìm nhị phân thay vì cache con trỏ như đề
 * xuất ban đầu, vì con trỏ ở đây không an toàn: lúc crossfade có **hai** lượt
 * lấy mẫu trên cùng một clip ở **hai** mốc thời gian khác nhau, xen kẽ nhau —
 * một con trỏ dùng chung sẽ bị hai lượt đó kéo qua kéo lại. Tìm nhị phân không
 * giữ trạng thái nên miễn nhiễm với chuyện đó, và vẫn hạ O(n) xuống O(log n).
 *
 * Giả định: `times` **đã sắp tăng dần**. Đúng theo dựng — nó đến từ track của
 * `THREE.AnimationClip`, mà three.js yêu cầu sắp sẵn.
 */
function timKhungKhoa(times: number[], time: number): number {
  let lo = 0;
  let hi = times.length;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (times[mid] >= time) hi = mid;
    else lo = mid + 1;
  }
  return lo;
}

function sampleTrack(track: RetargetedTrack, time: number): QuaternionTuple {
  if (track.times.length <= 1) return tupleAt(track.values, 0);
  const upper = timKhungKhoa(track.times, time);
  // Ngoài hai đầu: kẹp về khung khoá đầu hoặc cuối. Giữ đúng ngữ nghĩa cũ —
  // `findIndex` trả -1 khi vượt quá khung cuối, nay là `times.length`.
  if (upper >= track.times.length) return tupleAt(track.values, (track.times.length - 1) * 4);
  if (upper === 0) return tupleAt(track.values, 0);
  const lower = upper - 1;
  const span = track.times[upper] - track.times[lower];
  const weight = span > 0 ? (time - track.times[lower]) / span : 0;
  return slerpQuaternion(tupleAt(track.values, lower * 4), tupleAt(track.values, upper * 4), weight);
}

export function sampleRetargetedClip(clip: RetargetedClip, time: number, loop: boolean): Pose {
  const duration = Math.max(clip.duration, 0);
  const sampleTime = duration <= 0
    ? 0
    : loop
      ? ((time % duration) + duration) % duration
      : Math.min(Math.max(time, 0), duration);
  const pose: Pose = {};
  for (const [bone, track] of Object.entries(clip.tracks) as [ControlledBone, RetargetedTrack][]) {
    pose[bone] = quaternionToEuler(sampleTrack(track, sampleTime));
  }
  return pose;
}

export class MixamoRetargeter {
  private readonly bindings = new Map<ControlledBone, RetargetBinding>();

  constructor(bindings: RetargetBinding[]) {
    for (const binding of bindings) this.bindings.set(binding.bone, binding);
  }

  retargetQuaternion(bone: ControlledBone, sourcePose: QuaternionTuple): QuaternionTuple {
    const binding = this.bindings.get(bone);
    if (!binding) throw new Error(`Missing rest-pose binding for ${bone}`);

    const sourceDelta = multiplyQuaternion(invertQuaternion(binding.sourceRestLocal), sourcePose);
    const axisCorrection = multiplyQuaternion(
      invertQuaternion(binding.targetRestWorld),
      binding.sourceRestWorld,
    );
    const targetDelta = multiplyQuaternion(
      multiplyQuaternion(axisCorrection, sourceDelta),
      invertQuaternion(axisCorrection),
    );
    return multiplyQuaternion(binding.targetRestLocal, targetDelta);
  }

  retargetClip(source: SourceAnimationClip): RetargetedClip {
    const tracks: RetargetedClip["tracks"] = {};

    for (const track of source.tracks) {
      const separator = track.name.lastIndexOf(".");
      if (separator < 0 || track.name.slice(separator + 1) !== "quaternion") continue;
      const bone = mixamoBoneToVrm(track.name.slice(0, separator));
      if (!bone || !this.bindings.has(bone)) continue;

      const values: number[] = [];
      for (let index = 0; index + 3 < track.values.length; index += 4) {
        values.push(...this.retargetQuaternion(bone, tupleAt(track.values, index)));
      }
      tracks[bone] = { times: Array.from(track.times), values };
    }

    return { name: source.name, duration: source.duration, tracks };
  }
}
