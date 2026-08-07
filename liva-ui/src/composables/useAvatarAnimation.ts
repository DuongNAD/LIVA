/**
 * useAvatarAnimation.ts — Tư thế thân thể của avatar
 * ====================================================
 * Sinh tư thế đi/chạy/nhảy/vẫy bằng công thức (procedural) và áp thẳng lên
 * xương humanoid đã chuẩn hoá của VRM. Không cần file animation nào — VRoid
 * xuất ra model có rig nhưng KHÔNG kèm clip, nên nếu chờ clip ngoài thì nhân
 * vật đứng im.
 *
 * Chỗ này chỉ quản lý CHI (tay, chân) và độ nghiêng hông. Mặt (chớp mắt, biểu
 * cảm, khẩu hình), đầu và cột sống do use3DModel lo — hai bên không giẫm chân
 * nhau, nên thở và nhìn theo chuột vẫn chạy trong lúc đang bước.
 *
 * Clip ngoài (Mixamo, VRMA) khi có sẽ nạp qua registerClip() và được ưu tiên
 * hơn tư thế công thức cho đúng trạng thái đó.
 */
import type { VRM } from "@pixiv/three-vrm";
import { Vector3 } from "three";
import { sampleRetargetedClip, type RetargetedClip } from "./mixamoRetarget";
import { FootPlantIK } from "./footPlantIK";

export type LocomotionState = "idle" | "walk" | "run" | "jump";
export type GestureName = "wave" | "nod" | "shake";
export type AvatarClipState = LocomotionState | "wave" | "thinking";

/** Các xương mà lớp này điều khiển. Cố tình không có spine/head/neck. */
const CONTROLLED_BONES = [
  "leftUpperLeg", "leftLowerLeg", "leftFoot",
  "rightUpperLeg", "rightLowerLeg", "rightFoot",
  "leftUpperArm", "leftLowerArm",
  "rightUpperArm", "rightLowerArm",
  "hips",
] as const;

export type ControlledBone = (typeof CONTROLLED_BONES)[number];

/** Góc Euler (radian) theo thứ tự X, Y, Z */
type Euler3 = readonly [number, number, number];
export type Pose = Partial<Record<ControlledBone, Euler3>>;

/** Tần số bước mỗi giây cho từng trạng thái */
const STRIDE_HZ: Record<LocomotionState, number> = {
  idle: 0,
  walk: 1.05,
  run: 1.9,
  jump: 0,
};

/** Thời gian chuyển mượt giữa hai trạng thái (giây) */
const CROSSFADE_SECONDS = 0.28;

/**
 * Foot-plant có đang bật không — công tắc A/B của mục U30.
 *
 * Đọc `globalThis.LIVA_FOOT_PLANT` theo đúng lối `LIVA_ECO_MODE` /
 * `LIVA_AVATAR_DEMOTE_LEVEL` trong `use3DModel.ts`. So sánh với `false` chứ
 * không ép về boolean: chưa đặt biến ⇒ `undefined` ⇒ **bật**, nên hành vi mặc
 * định không đổi và không ai vô tình tắt nó bằng cách quên khai báo.
 */
export function footPlantEnabled(): boolean {
  return (globalThis as unknown as Record<string, unknown>).LIVA_FOOT_PLANT !== false;
}

export interface AvatarAnimationApi {
  setState: (state: LocomotionState) => void;
  /** Tốc độ hiện tại / tốc độ cực đại, dùng để đồng bộ nhịp và độ rộng bước. */
  setMotionWeight: (weight: number) => void;
  getState: () => LocomotionState;
  playGesture: (name: GestureName) => void;
  setInspecting: (active: boolean) => void;
  setThinking: (active: boolean) => void;
  /** Áp tư thế cho khung hình hiện tại. Gọi từ vòng lặp render. */
  update: (vrm: VRM | null, delta: number) => void;
  /** Đăng ký clip ngoài (Mixamo/VRMA) cho một trạng thái; ghi đè tư thế công thức. */
  registerClip: (state: AvatarClipState, clip: RetargetedClip) => void;
  hasClip: (state: AvatarClipState) => boolean;
  /** Ảnh chụp tư thế đang áp — dùng để kiểm thử, không dùng khi chạy thật. */
  debugPose: () => Pose;
  reset: () => void;
}

// ═══════════════════════════════════════════
//  Các tư thế công thức
// ═══════════════════════════════════════════

/**
 * Quy ước dấu. VRM nhìn theo +Z, tay trái ở phía +X, T-pose là tay dang ngang.
 *
 *   upperLeg.x  âm    → đùi đưa ra TRƯỚC
 *   lowerLeg.x  dương → gập gối ra SAU (đúng sinh lý, gối không bẻ ngược)
 *
 *   Vai xoay quanh Z. Tay trái xuất phát từ +X, quay góc θ đi tới (cos θ, sin θ):
 *     θ = 0      → dang ngang (T-pose)
 *     θ = −π/2   → buông thẳng xuống
 *     θ = +π/2   → giơ thẳng lên trời
 *   Tay phải xuất phát từ −X nên dấu ngược lại hoàn toàn.
 *
 *   ⚠ Dấu này từng bị đảo và làm nhân vật đứng giơ hai tay lên như đầu hàng.
 *   Có test khoá: ở idle, leftUpperArm.z phải ÂM và rightUpperArm.z phải DƯƠNG.
 */
const ARM_DOWN_LEFT = -1.22;
const ARM_DOWN_RIGHT = 1.22;

function poseIdle(t: number): Pose {
  // Đứng yên vẫn phải "sống": dồn trọng tâm rất nhẹ, chu kỳ dài để không thấy lặp
  const sway = Math.sin(t * 0.55) * 0.012;
  const armIdle = Math.sin(t * 0.42) * 0.02;
  return {
    hips: [0, 0, sway],
    leftUpperArm: [armIdle, 0, ARM_DOWN_LEFT],
    rightUpperArm: [-armIdle, 0, ARM_DOWN_RIGHT],
    leftLowerArm: [0, 0, -0.08],
    rightLowerArm: [0, 0, 0.08],
    leftUpperLeg: [0, 0, 0.02],
    rightUpperLeg: [0, 0, -0.02],
    leftLowerLeg: [0.02, 0, 0],
    rightLowerLeg: [0.02, 0, 0],
    leftFoot: [0, 0, 0],
    rightFoot: [0, 0, 0],
  };
}

/**
 * Một chu kỳ bước = 2π. Chân trái và chân phải lệch pha đúng nửa chu kỳ,
 * tay đánh ngược pha với chân cùng bên — đó là dáng đi tự nhiên của người.
 */
function poseStride(phase: number, intensity: number): Pose {
  const s = Math.sin(phase);
  const sOpposite = Math.sin(phase + Math.PI);

  // Gối chỉ gập khi chân đang ở pha sau (không bao giờ bẻ ngược)
  const kneeLeft = (Math.max(0, -s) * 1.15 + 0.05) * intensity;
  const kneeRight = (Math.max(0, -sOpposite) * 1.15 + 0.05) * intensity;

  const lean = 0.22 * intensity; // chạy thì chúi về trước nhiều hơn

  return {
    // Chậu xoay đối trọng và chuyển tải trái/phải đúng một lần mỗi chu kỳ bước.
    hips: [-lean, -s * 0.06 * intensity, s * 0.045 * intensity],
    leftUpperLeg: [-s * 0.72 * intensity, 0, 0.02],
    rightUpperLeg: [-sOpposite * 0.72 * intensity, 0, -0.02],
    leftLowerLeg: [kneeLeft, 0, 0],
    rightLowerLeg: [kneeRight, 0, 0],
    leftFoot: [s * 0.22 * intensity, 0, 0],
    rightFoot: [sOpposite * 0.22 * intensity, 0, 0],
    // Tay ngược pha chân cùng bên; đi càng nhanh tay càng hơi rời thân
    leftUpperArm: [-sOpposite * 0.55 * intensity, 0, ARM_DOWN_LEFT + 0.22 * intensity],
    rightUpperArm: [-s * 0.55 * intensity, 0, ARM_DOWN_RIGHT - 0.22 * intensity],
    leftLowerArm: [0, 0, -0.25 - 0.45 * intensity],
    rightLowerArm: [0, 0, 0.25 + 0.45 * intensity],
  };
}

/** Co chân, hơi dang tay — dùng suốt thời gian ở trên không */
function poseJump(): Pose {
  return {
    hips: [-0.12, 0, 0],
    leftUpperLeg: [-0.62, 0, 0.12],
    rightUpperLeg: [-0.62, 0, -0.12],
    leftLowerLeg: [0.95, 0, 0],
    rightLowerLeg: [0.95, 0, 0],
    leftFoot: [-0.25, 0, 0],
    rightFoot: [-0.25, 0, 0],
    // Nhảy thì tay dang rộng hơn lúc đứng (z gần 0 hơn = gần phương ngang)
    leftUpperArm: [-0.5, 0, -0.75],
    rightUpperArm: [-0.5, 0, 0.75],
    leftLowerArm: [0, 0, -0.35],
    rightLowerArm: [0, 0, 0.35],
  };
}

function smoothstep01(value: number): number {
  const clamped = Math.min(1, Math.max(0, value));
  return clamped * clamped * (3 - 2 * clamped);
}

function basePose(state: LocomotionState, t: number, phase: number, motionWeight: number): Pose {
  const motionEnvelope = smoothstep01(motionWeight);
  switch (state) {
    case "walk":
      return poseStride(phase, 0.72 * motionEnvelope);
    case "run":
      return poseStride(phase, motionEnvelope);
    case "jump":
      return poseJump();
    default:
      return poseIdle(t);
  }
}

// ═══════════════════════════════════════════
//  Trộn tư thế
// ═══════════════════════════════════════════

function lerp(a: number, b: number, t: number): number {
  return a + (b - a) * t;
}

/** Trộn hai tư thế; xương chỉ có ở một bên vẫn được nội suy từ 0. */
function blendPose(from: Pose, to: Pose, weight: number): Pose {
  const out: Record<string, Euler3> = {};
  for (const bone of CONTROLLED_BONES) {
    const a = from[bone];
    const b = to[bone];
    if (!a && !b) continue;
    const av = a ?? ([0, 0, 0] as const);
    const bv = b ?? ([0, 0, 0] as const);
    out[bone] = [
      lerp(av[0], bv[0], weight),
      lerp(av[1], bv[1], weight),
      lerp(av[2], bv[2], weight),
    ];
  }
  return out as Pose;
}

/** Cử chỉ được cộng THÊM vào tư thế nền, để vẫy tay được cả khi đang đi. */
function applyGesture(pose: Pose, gesture: GestureName | null, progress: number): Pose {
  if (!gesture) return pose;

  // Vào nhanh, giữ, rồi ra chậm — tránh giật ở hai đầu
  const envelope = progress < 0.18
    ? progress / 0.18
    : progress > 0.72
      ? Math.max(0, (1 - progress) / 0.28)
      : 1;

  if (gesture === "wave") {
    const swing = Math.sin(progress * Math.PI * 8) * 0.42 * envelope;
    // Giơ tay phải lên cạnh đầu: z chạy từ ARM_DOWN_RIGHT (buông) về âm (giơ lên)
    const raised = -1.5;
    return {
      ...pose,
      rightUpperArm: [-0.35 * envelope, 0, raised * envelope + ARM_DOWN_RIGHT * (1 - envelope)],
      rightLowerArm: [0, swing, 0.55 * envelope],
    };
  }

  if (gesture === "shake") {
    const shake = Math.sin(progress * Math.PI * 4) * 0.16 * envelope;
    const hips = pose.hips ?? ([0, 0, 0] as const);
    return { ...pose, hips: [hips[0], hips[1] + shake, hips[2]] };
  }

  // nod: gật bằng hông vì đầu do use3DModel giữ
  const nod = Math.sin(progress * Math.PI * 4) * 0.06 * envelope;
  const hips = pose.hips ?? ([0, 0, 0] as const);
  return { ...pose, hips: [hips[0] + nod, hips[1], hips[2]] };
}

function applyInspection(pose: Pose): Pose {
  const hips = pose.hips ?? ([0, 0, 0] as const);
  return {
    ...pose,
    hips: [Math.min(hips[0], -0.12), hips[1], hips[2]],
    // Tay phải vẫn giữ z DƯƠNG (đúng phía hạ tay của xương ở -X), còn xoay
    // mạnh quanh x để đưa cánh tay ra trước như đang chỉ vào kết quả.
    rightUpperArm: [-0.92, 0, 0.38],
    rightLowerArm: [-0.18, 0, 0.2],
  };
}

const GESTURE_SECONDS = 1.6;

export function useAvatarAnimation(): AvatarAnimationApi {
  let state: LocomotionState = "idle";
  let previousState: LocomotionState = "idle";
  let crossfade = 1; // 1 = đã chuyển xong sang `state`
  let clock = 0;
  let stridePhase = 0;
  let stateTime = 0;
  let previousStateTime = 0;
  let motionWeight = 1;

  let gesture: GestureName | null = null;
  let gestureTime = 0;
  let inspecting = false;
  let thinking = false;
  let thinkingWeight = 0;
  let thinkingTime = 0;

  let lastApplied: Pose = {};
  const clips = new Map<AvatarClipState, RetargetedClip>();
  const footPlantIk = new FootPlantIK();
  let footPlantVrm: VRM | null = null;
  let hipsRestPosition: readonly [number, number, number] | null = null;

  function setState(next: LocomotionState) {
    if (next === state) return;
    previousState = state;
    previousStateTime = stateTime;
    state = next;
    stateTime = 0;
    crossfade = 0;
  }

  function setMotionWeight(weight: number) {
    motionWeight = Math.min(1, Math.max(0, weight));
  }

  function playGesture(name: GestureName) {
    gesture = name;
    gestureTime = 0;
  }

  function setThinking(active: boolean) {
    if (active && !thinking) thinkingTime = 0;
    thinking = active;
  }

  function applyFootPlant(vrm: VRM, delta: number) {
    const humanoid = vrm.humanoid;
    const scene = vrm.scene;

    // ── Công tắc A/B cho mục U30 ─────────────────────────────────────────
    // Bù foot-plant hiện dịch cả `hips` theo phương NGANG để giữ bàn chân
    // đứng yên, và bị kẹp ở 0.14 rồi nhả khi đổi chân trụ — nghi là nguồn của
    // hiện tượng "khựng theo từng bước chân". Đây là phép thử rẻ nhất để xác
    // nhận: bật/tắt rồi nhìn, không cần dựng bộ đo.
    //
    //   LIVA_FOOT_PLANT = false   // trong console — tắt ngay, không build lại
    //   LIVA_FOOT_PLANT = true    // bật lại để so sánh
    //
    // Mặc định BẬT, nên không đặt gì thì hành vi y như trước.
    if (!footPlantEnabled()) {
      footPlantIk.reset();
      // Phải TRẢ `hips` về tư thế gốc. Chỉ `return` thôi thì nó đứng nguyên ở
      // lượt bù cuối cùng, và cái lệch đó đóng băng vĩnh viễn — trông như một
      // lỗi khác hẳn, đủ để làm hỏng chính phép A/B này.
      if (footPlantVrm === vrm && hipsRestPosition) {
        humanoid?.getNormalizedBoneNode("hips")?.position.set(...hipsRestPosition);
      }
      return;
    }

    if (!humanoid || !scene || !clips.has(state) || (state !== "walk" && state !== "run")) {
      footPlantIk.reset();
      return;
    }
    const hips = humanoid.getNormalizedBoneNode("hips");
    const leftFoot = humanoid.getNormalizedBoneNode("leftFoot");
    const rightFoot = humanoid.getNormalizedBoneNode("rightFoot");
    if (!hips?.parent || !leftFoot || !rightFoot) return;

    if (footPlantVrm !== vrm || !hipsRestPosition) {
      footPlantVrm = vrm;
      hipsRestPosition = [hips.position.x, hips.position.y, hips.position.z];
      footPlantIk.reset();
    }
    hips.position.set(...hipsRestPosition);
    scene.updateWorldMatrix(true, true);
    const leftWorld = leftFoot.getWorldPosition(new Vector3());
    const rightWorld = rightFoot.getWorldPosition(new Vector3());
    const correction = footPlantIk.update({
      state,
      leftFoot: leftWorld,
      rightFoot: rightWorld,
      delta,
    });
    if (correction.x === 0 && correction.y === 0 && correction.z === 0) return;

    const currentLocal = hips.parent.worldToLocal(leftWorld.clone());
    const correctedLocal = hips.parent.worldToLocal(leftWorld.clone().add(
      new Vector3(correction.x, correction.y, correction.z),
    ));
    correctedLocal.sub(currentLocal);
    hips.position.set(
      hipsRestPosition[0] + correctedLocal.x,
      hipsRestPosition[1] + correctedLocal.y,
      hipsRestPosition[2] + correctedLocal.z,
    );
    // U31(a): KHÔNG duyệt lại đồ thị ở đây.
    //
    // Trước đây có `scene.updateWorldMatrix(true, true)` lần thứ hai ngay chỗ
    // này. Nó thừa vì ba lý do độc lập, mỗi lý do đủ để bỏ:
    //
    //   1. Ngay sau `animation.update()`, vòng render còn chạy idle sway,
    //      blink, lookAt và micro-expression — tất cả đều GHI thêm rotation,
    //      nên ma trận vừa dựng lại đã cũ trước khi có ai đọc.
    //   2. Spring bone của three-vrm tự lo ma trận của chính nó
    //      (`_ancestors[i].updateWorldMatrix(...)` trong three-vrm-springbone),
    //      không dựa vào việc ai đó đã duyệt scene trước.
    //   3. `WebGLRenderer.render()` gọi `scene.updateMatrixWorld()` trước khi
    //      vẽ, nên kết quả nhìn thấy vẫn đúng.
    //
    // Lần duyệt CÒN LẠI ở trên thì cần thật: phải làm mới ma trận sau khi đặt
    // `hips` về tư thế gốc, trước khi đọc vị trí thế giới của hai bàn chân.
  }

  function update(vrm: VRM | null, delta: number) {
    clock += delta;
    const currentPlaybackRate = state === "walk" || state === "run" ? motionWeight : 1;
    const previousPlaybackRate = previousState === "walk" || previousState === "run"
      ? motionWeight
      : 1;
    stateTime += delta * currentPlaybackRate;
    previousStateTime += delta * previousPlaybackRate;

    // Nhịp bước tiến theo trạng thái ĐANG chuyển tới, nhưng vẫn chạy trong lúc
    // crossfade để chân không khựng giữa chừng khi đổi walk ↔ run.
    const hz = STRIDE_HZ[state] || STRIDE_HZ[previousState];
    if (hz > 0 && motionWeight > 0) {
      stridePhase = (stridePhase + delta * hz * motionWeight * Math.PI * 2) % (Math.PI * 2);
    }

    if (crossfade < 1) {
      crossfade = Math.min(1, crossfade + delta / CROSSFADE_SECONDS);
    }
    const thinkingTarget = thinking ? 1 : 0;
    if (thinkingWeight < thinkingTarget) {
      thinkingWeight = Math.min(thinkingTarget, thinkingWeight + delta / CROSSFADE_SECONDS);
    } else if (thinkingWeight > thinkingTarget) {
      thinkingWeight = Math.max(thinkingTarget, thinkingWeight - delta / CROSSFADE_SECONDS);
    }
    if (thinking || thinkingWeight > 0) thinkingTime += delta;

    const poseForState = (poseState: LocomotionState, time: number): Pose => {
      const procedural = basePose(poseState, clock, stridePhase, motionWeight);
      const clip = clips.get(poseState);
      if (!clip) return procedural;
      return { ...procedural, ...sampleRetargetedClip(clip, time, poseState !== "jump") };
    };
    const target = poseForState(state, stateTime);
    let pose = crossfade >= 1
      ? target
      : blendPose(poseForState(previousState, previousStateTime), target, crossfade);

    const thinkingClip = clips.get("thinking");
    if (thinkingClip && thinkingWeight > 0) {
      const thinkingPose = {
        ...pose,
        ...sampleRetargetedClip(thinkingClip, thinkingTime, true),
      };
      pose = blendPose(pose, thinkingPose, thinkingWeight);
    }

    if (inspecting) {
      if (thinkingClip && thinkingWeight <= 0) {
        pose = { ...pose, ...sampleRetargetedClip(thinkingClip, clock, true) };
      }
      pose = applyInspection(pose);
    }

    if (gesture) {
      gestureTime += delta;
      const gestureClip = gesture === "wave" ? clips.get("wave") : undefined;
      const gestureDuration = Math.max(gestureClip?.duration ?? GESTURE_SECONDS, Number.EPSILON);
      const progress = gestureTime / gestureDuration;
      if (progress >= 1) {
        gesture = null;
      } else if (gestureClip) {
        pose = { ...pose, ...sampleRetargetedClip(gestureClip, gestureTime, false) };
      } else {
        pose = applyGesture(pose, gesture, progress);
      }
    }

    lastApplied = pose;

    const humanoid = vrm?.humanoid;
    if (!humanoid) return;

    for (const bone of CONTROLLED_BONES) {
      const angles = pose[bone];
      if (!angles) continue;
      const node = humanoid.getNormalizedBoneNode(bone);
      if (!node) continue;
      node.rotation.x = angles[0];
      node.rotation.y = angles[1];
      node.rotation.z = angles[2];
    }
    applyFootPlant(vrm, delta);
  }

  function reset() {
    state = "idle";
    previousState = "idle";
    crossfade = 1;
    clock = 0;
    stridePhase = 0;
    stateTime = 0;
    previousStateTime = 0;
    motionWeight = 1;
    gesture = null;
    gestureTime = 0;
    inspecting = false;
    thinking = false;
    thinkingWeight = 0;
    thinkingTime = 0;
    footPlantIk.reset();
    footPlantVrm = null;
    hipsRestPosition = null;
    lastApplied = {};
  }

  return {
    setState,
    setMotionWeight,
    getState: () => state,
    playGesture,
    setInspecting: (active) => { inspecting = active; },
    setThinking,
    update,
    registerClip: (s, clip) => { clips.set(s, clip); },
    hasClip: (s) => clips.has(s),
    debugPose: () => lastApplied,
    reset,
  };
}
