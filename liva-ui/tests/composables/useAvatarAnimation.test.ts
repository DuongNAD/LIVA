import { describe, it, expect, vi } from "vitest";
import * as THREE from "three";
import {
  useAvatarAnimation,
  type ControlledBone,
  WALK_SPEED,
  RUN_SPEED,
  STRIDE_LENGTH,
  STRIDE_HZ,
} from "../../src/composables/useAvatarAnimation";

/** VRM giả: ghi lại góc xoay đã áp lên từng xương humanoid */
function makeVRM() {
  const nodes = new Map<string, { rotation: { x: number; y: number; z: number } }>();
  const getNormalizedBoneNode = vi.fn((name: string) => {
    if (!nodes.has(name)) nodes.set(name, { rotation: { x: 0, y: 0, z: 0 } });
    return nodes.get(name)!;
  });
  return { vrm: { humanoid: { getNormalizedBoneNode } } as never, nodes, getNormalizedBoneNode };
}

/** Chạy n giây với bước cố định */
function advance(anim: ReturnType<typeof useAvatarAnimation>, vrm: never | null, seconds: number) {
  for (let t = 0; t < seconds; t += 1 / 60) anim.update(vrm, 1 / 60);
}

describe("useAvatarAnimation", () => {
  it("mặc định ở trạng thái idle và đổi được trạng thái", () => {
    const anim = useAvatarAnimation();
    expect(anim.getState()).toBe("idle");
    anim.setState("walk");
    expect(anim.getState()).toBe("walk");
  });

  it("không vỡ khi chưa có VRM", () => {
    const anim = useAvatarAnimation();
    anim.setState("run");
    expect(() => advance(anim, null, 1)).not.toThrow();
    // Tư thế vẫn được tính, chỉ là chưa có chỗ để áp
    expect(Object.keys(anim.debugPose()).length).toBeGreaterThan(0);
  });

  it("áp góc xoay lên xương humanoid của VRM", () => {
    const anim = useAvatarAnimation();
    const { vrm, nodes, getNormalizedBoneNode } = makeVRM();

    anim.setState("walk");
    advance(anim, vrm, 0.6);

    expect(getNormalizedBoneNode).toHaveBeenCalledWith("leftUpperLeg");
    expect(getNormalizedBoneNode).toHaveBeenCalledWith("rightUpperArm");

    const leg = nodes.get("leftUpperLeg")!;
    expect(Math.abs(leg.rotation.x)).toBeGreaterThan(0.05); // chân có vung thật
  });

  it("buông tay xuống khi đứng yên, không giơ lên trời", () => {
    // Từng có lỗi đảo dấu làm nhân vật đứng giơ hai tay như đầu hàng.
    // VRM: tay trái ở +X nên z ÂM là hạ xuống; tay phải ở −X nên z DƯƠNG là hạ xuống.
    const anim = useAvatarAnimation();
    const { vrm, nodes } = makeVRM();
    advance(anim, vrm, 1);

    const left = nodes.get("leftUpperArm")!.rotation.z;
    const right = nodes.get("rightUpperArm")!.rotation.z;

    expect(left).toBeLessThan(-0.9);
    expect(right).toBeGreaterThan(0.9);
    // Hai tay phải đối xứng nhau
    expect(left).toBeCloseTo(-right, 5);
  });

  it("giữ tay buông trong suốt lúc đi và chạy", () => {
    for (const state of ["walk", "run"] as const) {
      const anim = useAvatarAnimation();
      const { vrm, nodes } = makeVRM();
      anim.setState(state);
      for (let i = 0; i < 300; i++) {
        anim.update(vrm, 1 / 60);
        expect(nodes.get("leftUpperArm")!.rotation.z).toBeLessThan(0);
        expect(nodes.get("rightUpperArm")!.rotation.z).toBeGreaterThan(0);
      }
    }
  });

  it("KHÔNG đụng vào cột sống, cổ và đầu — phần đó do use3DModel giữ", () => {
    const anim = useAvatarAnimation();
    const { vrm, getNormalizedBoneNode } = makeVRM();

    anim.setState("run");
    advance(anim, vrm, 1);

    const touched = getNormalizedBoneNode.mock.calls.map((c) => c[0]);
    expect(touched).not.toContain("spine");
    expect(touched).not.toContain("chest");
    expect(touched).not.toContain("neck");
    expect(touched).not.toContain("head");
  });

  it("hai chân bước đối pha nhau", () => {
    const anim = useAvatarAnimation();
    const { vrm, nodes } = makeVRM();
    anim.setState("walk");

    // Bỏ qua quãng crossfade để đọc đúng tư thế bước
    advance(anim, vrm, 0.5);

    let sawOpposite = false;
    for (let i = 0; i < 120; i++) {
      anim.update(vrm, 1 / 60);
      const left = nodes.get("leftUpperLeg")!.rotation.x;
      const right = nodes.get("rightUpperLeg")!.rotation.x;
      if (Math.abs(left) > 0.15 && Math.abs(right) > 0.15 && Math.sign(left) !== Math.sign(right)) {
        sawOpposite = true;
      }
    }
    expect(sawOpposite).toBe(true);
  });

  it("không bao giờ bẻ ngược đầu gối", () => {
    const anim = useAvatarAnimation();
    const { vrm, nodes } = makeVRM();

    for (const state of ["walk", "run", "jump"] as const) {
      anim.setState(state);
      for (let i = 0; i < 200; i++) {
        anim.update(vrm, 1 / 60);
        // Quy ước: lowerLeg.x dương = gập gối ra sau. Âm nghĩa là gối bẻ ngược.
        expect(nodes.get("leftLowerLeg")!.rotation.x).toBeGreaterThanOrEqual(0);
        expect(nodes.get("rightLowerLeg")!.rotation.x).toBeGreaterThanOrEqual(0);
      }
    }
  });

  it("chạy vung chân mạnh hơn đi bộ", () => {
    const peak = (state: "walk" | "run") => {
      const anim = useAvatarAnimation();
      const { vrm, nodes } = makeVRM();
      anim.setState(state);
      advance(anim, vrm, 0.5); // qua crossfade
      let max = 0;
      for (let i = 0; i < 240; i++) {
        anim.update(vrm, 1 / 60);
        max = Math.max(max, Math.abs(nodes.get("leftUpperLeg")!.rotation.x));
      }
      return max;
    };
    expect(peak("run")).toBeGreaterThan(peak("walk"));
  });

  it("chuyển trạng thái mượt chứ không nhảy cóc", () => {
    const anim = useAvatarAnimation();
    const { vrm, nodes } = makeVRM();

    anim.setState("run");
    advance(anim, vrm, 1);
    const beforeSwitch = nodes.get("leftUpperLeg")!.rotation.x;

    anim.setState("idle");
    anim.update(vrm, 1 / 60); // đúng một khung hình sau khi đổi
    const justAfter = nodes.get("leftUpperLeg")!.rotation.x;

    // Một khung hình chỉ được đi một phần nhỏ quãng đường về tư thế đứng
    expect(Math.abs(justAfter - beforeSwitch)).toBeLessThan(0.35);

    advance(anim, vrm, 1); // hết crossfade thì mới về gần 0
    expect(Math.abs(nodes.get("leftUpperLeg")!.rotation.x)).toBeLessThan(0.05);
  });

  it("vẫy tay tác động lên tay phải rồi tự kết thúc", () => {
    const anim = useAvatarAnimation();
    const { vrm, nodes } = makeVRM();

    advance(anim, vrm, 0.4);
    const armAtRest = nodes.get("rightUpperArm")!.rotation.z;

    anim.playGesture("wave");
    advance(anim, vrm, 0.8); // giữa cử chỉ
    const armWaving = nodes.get("rightUpperArm")!.rotation.z;
    expect(Math.abs(armWaving - armAtRest)).toBeGreaterThan(0.3);

    advance(anim, vrm, 1.4); // qua hết 1,6 s
    expect(nodes.get("rightUpperArm")!.rotation.z).toBeCloseTo(armAtRest, 1);
  });

  it("vẫy được cả khi đang đi", () => {
    const anim = useAvatarAnimation();
    const { vrm, nodes } = makeVRM();
    anim.setState("walk");
    advance(anim, vrm, 0.5);

    anim.playGesture("wave");
    advance(anim, vrm, 0.6);

    // Tay phải giơ lên vẫy (z âm = giơ lên), chân vẫn bước
    expect(nodes.get("rightUpperArm")!.rotation.z).toBeLessThan(-1.2);
    let legMoved = false;
    for (let i = 0; i < 60; i++) {
      anim.update(vrm, 1 / 60);
      if (Math.abs(nodes.get("leftUpperLeg")!.rotation.x) > 0.15) legMoved = true;
    }
    expect(legMoved).toBe(true);
  });

  it("giữ tư thế inspect nghiêng người và chỉ tay mà không đảo dấu tay chân", () => {
    const anim = useAvatarAnimation();
    const { vrm, nodes } = makeVRM();

    anim.setInspecting(true);
    advance(anim, vrm, 3);

    expect(nodes.get("hips")!.rotation.x).toBeLessThan(-0.08);
    expect(nodes.get("rightUpperArm")!.rotation.x).toBeLessThan(-0.6);
    expect(nodes.get("leftUpperArm")!.rotation.z).toBeLessThan(0);
    expect(nodes.get("rightUpperArm")!.rotation.z).toBeGreaterThan(0);
    expect(nodes.get("leftLowerLeg")!.rotation.x).toBeGreaterThanOrEqual(0);
    expect(nodes.get("rightLowerLeg")!.rotation.x).toBeGreaterThanOrEqual(0);
  });

  it("lắc người phủ định khi tool lỗi rồi tự trở về tư thế nền", () => {
    const anim = useAvatarAnimation();
    const { vrm, nodes } = makeVRM();

    anim.playGesture("shake");
    advance(anim, vrm, 0.45);
    expect(Math.abs(nodes.get("hips")!.rotation.y)).toBeGreaterThan(0.04);

    advance(anim, vrm, 1.7);
    expect(Math.abs(nodes.get("hips")!.rotation.y)).toBeLessThan(0.01);
  });

  it("reset đưa về idle", () => {
    const anim = useAvatarAnimation();
    anim.setState("run");
    anim.playGesture("wave");
    anim.reset();
    expect(anim.getState()).toBe("idle");
    expect(anim.debugPose()).toEqual({});
  });

  it("nhận đăng ký clip ngoài cho từng trạng thái", () => {
    const anim = useAvatarAnimation();
    expect(anim.hasClip("walk")).toBe(false);
    anim.registerClip("walk", { name: "mixamo-walk", duration: 1, tracks: {} });
    expect(anim.hasClip("walk")).toBe(true);
    expect(anim.hasClip("run")).toBe(false);
  });

  it("ưu tiên clip Mixamo và crossfade từ tư thế công thức thay vì giật khung hình", () => {
    const anim = useAvatarAnimation();
    const { vrm, nodes } = makeVRM();
    const angle = 1.2;
    const half = angle / 2;
    anim.registerClip("walk", {
      name: "mixamo-walk",
      duration: 1,
      tracks: {
        leftUpperLeg: {
          times: [0, 1],
          values: [Math.sin(half), 0, 0, Math.cos(half), Math.sin(half), 0, 0, Math.cos(half)],
        },
      },
    });

    advance(anim, vrm, 0.3);
    const before = nodes.get("leftUpperLeg")!.rotation.x;
    anim.setState("walk");
    anim.update(vrm, 1 / 60);
    const firstFrame = nodes.get("leftUpperLeg")!.rotation.x;
    expect(Math.abs(firstFrame - before)).toBeLessThan(0.2);

    advance(anim, vrm, 0.4);
    expect(nodes.get("leftUpperLeg")!.rotation.x).toBeCloseTo(angle, 2);
  });

  it("dùng clip wave đã retarget thay cho cử chỉ công thức khi có sẵn", () => {
    const anim = useAvatarAnimation();
    const { vrm, nodes } = makeVRM();
    const angle = -0.8;
    const half = angle / 2;
    anim.registerClip("wave", {
      name: "mixamo-wave",
      duration: 1,
      tracks: {
        rightUpperArm: {
          times: [0, 1],
          values: [Math.sin(half), 0, 0, Math.cos(half), Math.sin(half), 0, 0, Math.cos(half)],
        },
      },
    });

    anim.playGesture("wave");
    advance(anim, vrm, 0.4);

    expect(nodes.get("rightUpperArm")!.rotation.x).toBeCloseTo(angle, 2);
  });

  it("áp foot-plant IK cho clip walk để chân trụ không trượt khi root tiến lên", () => {
    const scene = new THREE.Group();
    const hips = new THREE.Object3D();
    const leftFoot = new THREE.Object3D();
    const rightFoot = new THREE.Object3D();
    leftFoot.position.set(-0.1, 0, 0);
    rightFoot.position.set(0.1, 0.08, 0);
    scene.add(hips);
    hips.add(leftFoot, rightFoot);
    const nodes = { hips, leftFoot, rightFoot };
    const vrm = {
      scene,
      humanoid: {
        getNormalizedBoneNode: (bone: keyof typeof nodes) => nodes[bone] ?? null,
      },
    } as never;
    const anim = useAvatarAnimation();
    anim.registerClip("walk", {
      name: "walk",
      duration: 1,
      tracks: { leftFoot: { times: [0], values: [0, 0, 0, 1] } },
    });
    anim.setState("walk");

    anim.update(vrm, 1 / 60);
    scene.position.x += 0.05;
    for (let frame = 0; frame < 20; frame++) anim.update(vrm, 1 / 60);

    // U30: Horizontal compensation is eliminated on hips to prevent sawtooth stutter,
    // confining FootPlant correction to vertical damping only.
    expect(hips.position.x).toBeCloseTo(0, 5);
  });

  it("chỉ duyệt lại đồ thị MỘT lần mỗi khung hình khi đặt bàn chân", () => {
    // U31(a): trước đây `applyFootPlant` gọi `scene.updateWorldMatrix(true, true)`
    // HAI lần — một trước khi đo bàn chân, một sau khi ghi `hips.position`.
    // Trên đồ thị 333 node của Liva.vrm đó là ~666 lần cập nhật ma trận mỗi
    // khung hình chỉ để đặt bàn chân. Lần thứ hai thừa: idle sway/blink/lookAt
    // chạy ngay sau đó và ghi đè rotation, spring bone của three-vrm tự lo ma
    // trận của nó, còn renderer thì gọi `updateMatrixWorld()` trước khi vẽ.
    const scene = new THREE.Group();
    const hips = new THREE.Object3D();
    const leftFoot = new THREE.Object3D();
    const rightFoot = new THREE.Object3D();
    leftFoot.position.set(-0.1, 0, 0);
    rightFoot.position.set(0.1, 0.08, 0);
    scene.add(hips);
    hips.add(leftFoot, rightFoot);
    const nodes = { hips, leftFoot, rightFoot };
    const vrm = {
      scene,
      humanoid: {
        getNormalizedBoneNode: (bone: keyof typeof nodes) => nodes[bone] ?? null,
      },
    } as never;
    const anim = useAvatarAnimation();
    anim.registerClip("walk", {
      name: "walk",
      duration: 1,
      tracks: { leftFoot: { times: [0], values: [0, 0, 0, 1] } },
    });
    anim.setState("walk");

    // Cho tích luỹ sai lệch để nhánh "có correction" thật sự chạy — nhánh đó
    // mới là chỗ từng có lần duyệt thứ hai.
    anim.update(vrm, 1 / 60);
    scene.position.x += 0.05;
    for (let frame = 0; frame < 10; frame++) anim.update(vrm, 1 / 60);

    const spy = vi.spyOn(scene, "updateWorldMatrix");
    anim.update(vrm, 1 / 60);

    // Đếm riêng lần duyệt TOÀN CÂY `(true, true)` — đó mới là lần đắt, chạm cả
    // 333 node. `getWorldPosition` cũng gọi `updateWorldMatrix`, nhưng dạng
    // `(true, false)`: chỉ đi ngược lên chuỗi cha, rẻ và không tránh được.
    const toanCay = spy.mock.calls.filter(([parents, children]) => parents === true && children === true);
    expect(toanCay).toHaveLength(1);
    spy.mockRestore();
  });

  it("LIVA_FOOT_PLANT = false tắt bù và trả hips về tư thế gốc", () => {
    const scene = new THREE.Group();
    const hips = new THREE.Object3D();
    const leftFoot = new THREE.Object3D();
    const rightFoot = new THREE.Object3D();
    leftFoot.position.set(-0.1, 0, 0);
    rightFoot.position.set(0.1, 0.08, 0);
    scene.add(hips);
    hips.add(leftFoot, rightFoot);
    const nodes = { hips, leftFoot, rightFoot };
    const vrm = {
      scene,
      humanoid: {
        getNormalizedBoneNode: (bone: keyof typeof nodes) => nodes[bone] ?? null,
      },
    } as never;
    const anim = useAvatarAnimation();
    anim.registerClip("walk", {
      name: "walk",
      duration: 1,
      tracks: { leftFoot: { times: [0], values: [0, 0, 0, 1] } },
    });
    anim.setState("walk");

    // Bật (mặc định) — hips không bị kéo lùi theo phương ngang (U30).
    anim.update(vrm, 1 / 60);
    scene.position.x += 0.05;
    for (let frame = 0; frame < 20; frame++) anim.update(vrm, 1 / 60);
    expect(hips.position.x).toBeCloseTo(0, 5);

    // Tắt — hips phải TRỞ VỀ 0, không đóng băng ở lượt bù cuối. Đây là phần
    // dễ sót nhất: chỉ `return` sớm thì độ lệch cuối cùng nằm lại vĩnh viễn và
    // trông như một lỗi khác hẳn, đủ để làm hỏng chính phép A/B.
    vi.stubGlobal("LIVA_FOOT_PLANT", false);
    try {
      anim.update(vrm, 1 / 60);
      expect(hips.position.x).toBeCloseTo(0, 5);

      // Và nó phải ĐỨNG YÊN ở đó dù root tiếp tục tiến.
      for (let frame = 0; frame < 20; frame++) {
        scene.position.x += 0.01;
        anim.update(vrm, 1 / 60);
      }
      expect(hips.position.x).toBeCloseTo(0, 5);
    } finally {
      vi.unstubAllGlobals();
    }
  });

  it("blends the thinking clip under the inspect pointing pose", () => {
    const anim = useAvatarAnimation();
    const { vrm, nodes } = makeVRM();
    const angle = -0.55;
    const half = angle / 2;
    anim.registerClip("thinking", {
      name: "thinking",
      duration: 1,
      tracks: {
        leftUpperArm: {
          times: [0],
          values: [Math.sin(half), 0, 0, Math.cos(half)],
        },
      },
    });
    anim.setInspecting(true);

    advance(anim, vrm, 0.4);

    expect(nodes.get("leftUpperArm")!.rotation.x).toBeCloseTo(angle, 2);
    expect(nodes.get("rightUpperArm")!.rotation.x).toBeLessThan(-0.6);
  });

  it("plays the thinking arm clip without forcing the inspect pointing pose", () => {
    const anim = useAvatarAnimation();
    const { vrm, nodes } = makeVRM();
    const angle = -0.72;
    const half = angle / 2;
    anim.registerClip("thinking", {
      name: "thinking",
      duration: 1,
      tracks: {
        rightUpperArm: {
          times: [0],
          values: [Math.sin(half), 0, 0, Math.cos(half)],
        },
      },
    });

    anim.setThinking(true);
    advance(anim, vrm, 0.4);

    expect(nodes.get("rightUpperArm")!.rotation.x).toBeCloseTo(angle, 2);
    expect(nodes.get("rightUpperArm")!.rotation.z).toBeCloseTo(0, 2);
  });

  it("crossfades smoothly out of the thinking clip", () => {
    const anim = useAvatarAnimation();
    const { vrm, nodes } = makeVRM();
    const angle = -0.72;
    const half = angle / 2;
    anim.registerClip("thinking", {
      name: "thinking",
      duration: 1,
      tracks: {
        rightUpperArm: {
          times: [0],
          values: [Math.sin(half), 0, 0, Math.cos(half)],
        },
      },
    });
    anim.setThinking(true);
    advance(anim, vrm, 0.4);
    const thinkingPose = nodes.get("rightUpperArm")!.rotation.x;

    anim.setThinking(false);
    anim.update(vrm, 1 / 60);

    expect(Math.abs(nodes.get("rightUpperArm")!.rotation.x - thinkingPose)).toBeLessThan(0.1);
    advance(anim, vrm, 0.4);
    expect(nodes.get("rightUpperArm")!.rotation.x).toBeCloseTo(0, 1);
  });

  it("crossfades smoothly across clip walk, clip run and procedural idle", () => {
    const anim = useAvatarAnimation();
    const { vrm, nodes } = makeVRM();
    const constantClip = (name: string, angle: number) => {
      const half = angle / 2;
      return {
        name,
        duration: 1,
        tracks: {
          leftUpperLeg: {
            times: [0],
            values: [Math.sin(half), 0, 0, Math.cos(half)],
          },
        },
      };
    };
    anim.registerClip("walk", constantClip("walk", 0.9));
    anim.registerClip("run", constantClip("run", -0.9));
    anim.setState("walk");
    advance(anim, vrm, 0.5);

    const walkPose = nodes.get("leftUpperLeg")!.rotation.x;
    anim.setState("run");
    anim.update(vrm, 1 / 60);
    const firstRunFrame = nodes.get("leftUpperLeg")!.rotation.x;
    expect(Math.abs(firstRunFrame - walkPose)).toBeLessThan(0.2);
    advance(anim, vrm, 0.4);

    const runPose = nodes.get("leftUpperLeg")!.rotation.x;
    anim.setState("idle");
    anim.update(vrm, 1 / 60);
    expect(Math.abs(nodes.get("leftUpperLeg")!.rotation.x - runPose)).toBeLessThan(0.2);
  });

  it("scales procedural stride amplitude with real locomotion speed", () => {
    const peakStride = (motion: number) => {
      const anim = useAvatarAnimation();
      const { vrm, nodes } = makeVRM();
      anim.setState("walk");
      anim.setMotionWeight(motion);
      advance(anim, vrm, 0.5);
      let peak = 0;
      for (let i = 0; i < 120; i++) {
        anim.update(vrm, 1 / 60);
        peak = Math.max(peak, Math.abs(nodes.get("leftUpperLeg")!.rotation.x));
      }
      return peak;
    };

    expect(peakStride(0.2)).toBeLessThan(peakStride(1) * 0.45);
  });

  it("shifts and counter-rotates the pelvis once per stride cycle", () => {
    const anim = useAvatarAnimation();
    const { vrm, nodes } = makeVRM();
    anim.setState("walk");
    advance(anim, vrm, 0.5);

    let peakYaw = 0;
    let lateralDirectionChanges = 0;
    let previousDirection = 0;
    for (let i = 0; i < 57; i++) {
      anim.update(vrm, 1 / 60);
      const hips = nodes.get("hips")!.rotation;
      peakYaw = Math.max(peakYaw, Math.abs(hips.y));
      const direction = Math.abs(hips.z) < 0.002 ? previousDirection : Math.sign(hips.z);
      if (previousDirection !== 0 && direction !== previousDirection) lateralDirectionChanges++;
      previousDirection = direction;
    }

    expect(peakYaw).toBeGreaterThan(0.015);
    expect(lateralDirectionChanges).toBeGreaterThanOrEqual(1);
    expect(lateralDirectionChanges).toBeLessThanOrEqual(2);
  });

  it("chỉ điều khiển đúng bộ xương đã khai báo", () => {
    const anim = useAvatarAnimation();
    const { vrm, getNormalizedBoneNode } = makeVRM();
    anim.setState("walk");
    advance(anim, vrm, 1);

    const allowed: ControlledBone[] = [
      "leftUpperLeg", "leftLowerLeg", "leftFoot",
      "rightUpperLeg", "rightLowerLeg", "rightFoot",
      "leftUpperArm", "leftLowerArm",
      "rightUpperArm", "rightLowerArm",
      "hips",
    ];
    for (const call of getNormalizedBoneNode.mock.calls) {
      expect(allowed).toContain(call[0] as ControlledBone);
    }
  });

  describe("Distance-Based Stride Phase Calibration (Slice A2)", () => {
    it("độ dài bước walk được chuẩn hoá đúng WALK_SPEED / 1.05", () => {
      expect(STRIDE_LENGTH.walk).toBeCloseTo(WALK_SPEED / 1.05, 6);
      expect(STRIDE_LENGTH.run).toBeCloseTo(RUN_SPEED / 1.9, 6);
      expect(STRIDE_HZ.walk).toBe(1.05);
      expect(STRIDE_HZ.run).toBe(1.9);
    });

    it("stridePhase đóng băng khi currentSpeed == 0 (motionWeight == 0) dù state là walk", () => {
      const anim = useAvatarAnimation();
      anim.setState("walk");
      anim.setMotionWeight(0);
      expect(anim.getStridePhase()).toBe(0);

      anim.update(null, 0.5);
      expect(anim.getStridePhase()).toBe(0);
      expect(anim.stridePhase).toBe(0);
    });

    it("tần số bước ở steady-state WALK_SPEED (motionWeight = 1) khớp chính xác baseline 1.05 Hz", () => {
      const anim = useAvatarAnimation();
      anim.setState("walk");
      anim.setMotionWeight(1);

      // 1 giây ở 1.05 Hz = 1.05 * 2π = 2.1π radians = 0.1π mod 2π
      const dt = 1 / 60;
      for (let i = 0; i < 60; i++) {
        anim.update(null, dt);
      }
      const expectedPhase = (1.05 * Math.PI * 2) % (Math.PI * 2);
      expect(anim.getStridePhase()).toBeCloseTo(expectedPhase, 4);
    });

    it("stridePhase tăng tỉ lệ thuận với quãng đường di chuyển và motionWeight", () => {
      const animHalf = useAvatarAnimation();
      const animFull = useAvatarAnimation();
      animHalf.setState("walk");
      animHalf.setMotionWeight(0.5);
      animFull.setState("walk");
      animFull.setMotionWeight(1.0);

      animHalf.update(null, 0.1);
      animFull.update(null, 0.1);

      expect(animHalf.getStridePhase()).toBeCloseTo(animFull.getStridePhase() * 0.5, 5);
      expect(animHalf.getMotionWeight()).toBe(0.5);
      expect(animHalf.motionWeight).toBe(0.5);
      expect(animFull.getMotionWeight()).toBe(1.0);
    });

    it("reset() trả stridePhase về 0", () => {
      const anim = useAvatarAnimation();
      anim.setState("walk");
      anim.setMotionWeight(1);
      anim.update(null, 0.5);
      expect(anim.getStridePhase()).toBeGreaterThan(0);

      anim.reset();
      expect(anim.getStridePhase()).toBe(0);
      expect(anim.getMotionWeight()).toBe(1);
    });

    it("stridePhase KHÔNG tăng khi ở idle sau khi đã kết thúc crossfade dù motionWeight > 0", () => {
      const anim = useAvatarAnimation();
      anim.setState("run");
      anim.setMotionWeight(1);
      anim.update(null, 0.5);
      const phaseAfterRun = anim.getStridePhase();
      expect(phaseAfterRun).toBeGreaterThan(0);

      // Chuyển sang idle và cho trôi qua toàn bộ thời gian crossfade (0.28s)
      anim.setState("idle");
      anim.update(null, 0.5);
      const phaseAfterIdle = anim.getStridePhase();

      // Đặt motionWeight > 0 khi đang ở idle và cập nhật tiếp
      anim.setMotionWeight(0.8);
      anim.update(null, 0.5);
      // stridePhase phải giữ nguyên, không được tiếp tục quay theo previousState (run)
      expect(anim.getStridePhase()).toBe(phaseAfterIdle);
    });

    it("stridePhase KHÔNG tăng khi ở trạng thái jump", () => {
      const anim = useAvatarAnimation();
      anim.setState("walk");
      anim.setMotionWeight(1);
      anim.update(null, 0.2);
      const phaseBeforeJump = anim.getStridePhase();

      anim.setState("jump");
      // Trôi qua crossfade
      anim.update(null, 0.3);
      const phaseInJump = anim.getStridePhase();

      anim.update(null, 0.5);
      expect(anim.getStridePhase()).toBe(phaseInJump);
    });

    it("setMotionWeight xử lý an toàn các giá trị biên và NaN/Infinity", () => {
      const anim = useAvatarAnimation();

      anim.setMotionWeight(NaN);
      expect(anim.getMotionWeight()).toBe(0);

      anim.setMotionWeight(-0.5);
      expect(anim.getMotionWeight()).toBe(0);

      anim.setMotionWeight(1.5);
      expect(anim.getMotionWeight()).toBe(1);

      anim.setMotionWeight(Infinity);
      expect(anim.getMotionWeight()).toBe(0);
    });

    it("update() với delta âm hoặc NaN không làm hỏng đồng hồ nội bộ", () => {
      const anim = useAvatarAnimation();
      anim.setState("walk");
      anim.setMotionWeight(1);

      expect(() => {
        anim.update(null, -0.1);
        anim.update(null, NaN);
        anim.update(null, 5.0); // very large jump
      }).not.toThrow();

      expect(Number.isFinite(anim.getStridePhase())).toBe(true);
      expect(Number.isFinite(anim.getMotionWeight())).toBe(true);
    });

    it("chuyển tiếp run <-> walk duy trì tần số bước mượt mà không bị gián đoạn", () => {
      const anim = useAvatarAnimation();
      anim.setState("run");
      anim.setMotionWeight(1);
      anim.update(null, 1.0); // ổn định ở run

      // Đổi sang walk và đo tốc độ tăng stridePhase trong suốt crossfade
      anim.setState("walk");
      let prevPhase = anim.getStridePhase();
      const dt = 1 / 60;
      for (let step = 0; step < 20; step++) {
        anim.update(null, dt);
        const curPhase = anim.getStridePhase();
        const diff = (curPhase - prevPhase + Math.PI * 2) % (Math.PI * 2);
        // Tần số tương đương = diff / (dt * 2π). Phải nằm trong khoảng [1.0, 2.0] Hz
        const freqHz = diff / (dt * Math.PI * 2);
        expect(freqHz).toBeGreaterThanOrEqual(1.0);
        expect(freqHz).toBeLessThanOrEqual(2.0);
        prevPhase = curPhase;
      }
    });

    it("chuyển từ walk sang idle trả lại vị trí hips gốc cho footPlant", () => {
      const scene = new THREE.Group();
      const hips = new THREE.Object3D();
      const leftFoot = new THREE.Object3D();
      const rightFoot = new THREE.Object3D();
      leftFoot.position.set(-0.1, 0, 0);
      rightFoot.position.set(0.1, 0.08, 0);
      scene.add(hips);
      hips.add(leftFoot, rightFoot);
      const nodes = { hips, leftFoot, rightFoot };
      const vrm = {
        scene,
        humanoid: {
          getNormalizedBoneNode: (bone: keyof typeof nodes) => nodes[bone] ?? null,
        },
      } as never;

      const anim = useAvatarAnimation();
      anim.registerClip("walk", {
        name: "walk",
        duration: 1,
        tracks: { leftFoot: { times: [0], values: [0, 0, 0, 1] } },
      });

      anim.setState("walk");
      anim.update(vrm, 1 / 60);
      scene.position.x += 0.05;
      for (let frame = 0; frame < 10; frame++) anim.update(vrm, 1 / 60);

      // Chuyển sang idle
      anim.setState("idle");
      anim.update(vrm, 1 / 60);

      // Hips position phải được trả về rest position (0, 0, 0)
      expect(hips.position.x).toBeCloseTo(0, 5);
      expect(hips.position.y).toBeCloseTo(0, 5);
      expect(hips.position.z).toBeCloseTo(0, 5);
    });

    it("tự nhiên gập cẳng tay khi vung tay ra trước và ngửa/duỗi mắt cá chân khi bước", () => {
      const anim = useAvatarAnimation();
      const { vrm, nodes } = makeVRM();
      anim.setState("walk");
      advance(anim, vrm, 0.5); // qua crossfade

      let sawElbowFlex = false;
      let sawAnklePitchUp = false;
      let sawAnklePitchDown = false;

      for (let i = 0; i < 120; i++) {
        anim.update(vrm, 1 / 60);
        const leftElbow = nodes.get("leftLowerArm")!.rotation.x;
        const rightElbow = nodes.get("rightLowerArm")!.rotation.x;
        const leftFoot = nodes.get("leftFoot")!.rotation.x;

        if (leftElbow < -0.1 || rightElbow < -0.1) sawElbowFlex = true;
        if (leftFoot > 0.05) sawAnklePitchUp = true;
        if (leftFoot < -0.05) sawAnklePitchDown = true;
      }

      expect(sawElbowFlex).toBe(true);
      expect(sawAnklePitchUp).toBe(true);
      expect(sawAnklePitchDown).toBe(true);
    });

    it("nhấp nhô trọng tâm hông (pelvis vertical bobbing) 2x theo nhịp bước trong procedural walk", () => {
      const scene = new THREE.Group();
      const hips = new THREE.Object3D();
      scene.add(hips);
      const nodes = { hips };
      const vrm = {
        scene,
        humanoid: {
          getNormalizedBoneNode: (bone: keyof typeof nodes) => nodes[bone] ?? null,
        },
      } as never;

      const anim = useAvatarAnimation();
      anim.setState("walk");

      const yPositions: number[] = [];
      for (let i = 0; i < 120; i++) {
        anim.update(vrm, 1 / 60);
        yPositions.push(hips.position.y);
      }

      const minY = Math.min(...yPositions);
      const maxY = Math.max(...yPositions);
      expect(minY).toBeLessThan(0); // Có nhấp nhô trọng tâm xuống
      expect(maxY).toBeCloseTo(0, 3); // Đỉnh nhịp chạm lại rest Y
    });

    it("áp tư thế xách gáy (dangle) co chân và quắp tay khi nhấc bổng", () => {
      const anim = useAvatarAnimation();
      const { vrm, nodes } = makeVRM();

      anim.setState("dangle");
      advance(anim, vrm, 0.5);

      const leftUpperLeg = nodes.get("leftUpperLeg")!.rotation.x;
      const leftLowerLeg = nodes.get("leftLowerLeg")!.rotation.x;
      const leftLowerArm = nodes.get("leftLowerArm")!.rotation.x;

      expect(leftUpperLeg).toBeLessThan(-0.2); // Đùi co về trước
      expect(leftLowerLeg).toBeGreaterThan(0.5); // Gối gập tự nhiên
      expect(leftLowerArm).toBeLessThan(-0.2); // Tay quắp trước ngực
    });
  });
});
