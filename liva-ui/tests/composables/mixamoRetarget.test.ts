import { describe, expect, it } from "vitest";
import {
  MixamoRetargeter,
  mixamoBoneToVrm,
  sampleRetargetedClip,
  type QuaternionTuple,
  type RetargetBinding,
} from "../../src/composables/mixamoRetarget";

const axisAngle = (axis: "x" | "y" | "z", radians: number): QuaternionTuple => {
  const half = radians / 2;
  const sin = Math.sin(half);
  return [axis === "x" ? sin : 0, axis === "y" ? sin : 0, axis === "z" ? sin : 0, Math.cos(half)];
};

const identity: QuaternionTuple = [0, 0, 0, 1];

describe("MixamoRetargeter", () => {
  it("maps mixamorig names only to the limb and hips bones owned by avatar animation", () => {
    expect(mixamoBoneToVrm("mixamorig:Hips")).toBe("hips");
    expect(mixamoBoneToVrm("mixamorigLeftUpLeg")).toBe("leftUpperLeg");
    expect(mixamoBoneToVrm("mixamorig:RightForeArm")).toBe("rightLowerArm");
    expect(mixamoBoneToVrm("mixamorig:Spine")).toBeNull();
    expect(mixamoBoneToVrm("mixamorig:Head")).toBeNull();
  });

  it("converts the source quaternion delta through both source and target rest-pose axes", () => {
    const binding: RetargetBinding = {
      bone: "leftUpperArm",
      sourceRestLocal: identity,
      sourceRestWorld: axisAngle("z", Math.PI / 2),
      targetRestLocal: identity,
      targetRestWorld: identity,
    };
    const retargeter = new MixamoRetargeter([binding]);

    const actual = retargeter.retargetQuaternion("leftUpperArm", axisAngle("x", Math.PI / 2));
    const expected = axisAngle("y", Math.PI / 2);
    const alignment = Math.abs(actual.reduce((sum, value, index) => sum + value * expected[index], 0));

    expect(alignment).toBeCloseTo(1, 5);
  });

  it("drops spine and head tracks while retargeting quaternion keyframes", () => {
    const retargeter = new MixamoRetargeter([
      {
        bone: "leftUpperArm",
        sourceRestLocal: identity,
        sourceRestWorld: identity,
        targetRestLocal: identity,
        targetRestWorld: identity,
      },
    ]);

    const clip = retargeter.retargetClip({
      name: "mixamo-wave",
      duration: 1,
      tracks: [
        { name: "mixamorig:LeftArm.quaternion", times: [0, 1], values: [...identity, ...axisAngle("x", 1)] },
        { name: "mixamorig:Spine.quaternion", times: [0, 1], values: [...identity, ...identity] },
        { name: "mixamorig:Head.quaternion", times: [0, 1], values: [...identity, ...identity] },
      ],
    });

    expect(Object.keys(clip.tracks)).toEqual(["leftUpperArm"]);
    expect(clip.tracks.leftUpperArm?.times).toEqual([0, 1]);
  });

  it("samples quaternion keyframes with spherical interpolation and looping", () => {
    const halfway = sampleRetargetedClip({
      name: "walk",
      duration: 1,
      tracks: {
        leftUpperLeg: {
          times: [0, 1],
          values: [...identity, ...axisAngle("x", Math.PI)],
        },
      },
    }, 0.5, true);
    const looped = sampleRetargetedClip({
      name: "walk",
      duration: 1,
      tracks: {
        leftUpperLeg: {
          times: [0, 1],
          values: [...identity, ...axisAngle("x", Math.PI)],
        },
      },
    }, 1.5, true);

    expect(halfway.leftUpperLeg?.[0]).toBeCloseTo(Math.PI / 2, 4);
    expect(looped.leftUpperLeg?.[0]).toBeCloseTo(Math.PI / 2, 4);
  });

  // ── U31(c): tìm khung khoá bằng nhị phân thay vì quét tuyến tính ──────────
  // `sampleTrack` cũ dùng `Array.findIndex`, quét từ đầu cho MỖI xương MỖI
  // khung hình. Đổi sang tìm nhị phân phải giữ **nguyên** ngữ nghĩa ở cả bốn
  // biên dưới đây — đây là chỗ dễ lệch một đơn vị nhất.
  describe("tìm khung khoá — biên phải giữ nguyên ngữ nghĩa cũ", () => {
    /** Clip 5 khung khoá, quay quanh trục x từ 0 tới π theo bốn chặng đều. */
    const clip = {
      name: "walk",
      duration: 4,
      tracks: {
        leftUpperLeg: {
          times: [0, 1, 2, 3, 4],
          values: [
            ...axisAngle("x", 0),
            ...axisAngle("x", Math.PI / 4),
            ...axisAngle("x", Math.PI / 2),
            ...axisAngle("x", (3 * Math.PI) / 4),
            ...axisAngle("x", Math.PI),
          ],
        },
      },
    };
    const goc = (time: number) => sampleRetargetedClip(clip, time, false).leftUpperLeg?.[0];

    it("trước khung khoá đầu ⇒ kẹp về khung đầu", () => {
      expect(goc(-5)).toBeCloseTo(0, 4);
    });

    it("sau khung khoá cuối ⇒ kẹp về khung cuối", () => {
      // Không lặp nên thời gian bị kẹp về `duration`; giá trị là khung cuối.
      expect(goc(99)).toBeCloseTo(Math.PI, 4);
    });

    it("trúng đúng một khung khoá ⇒ trả chính giá trị đó", () => {
      expect(goc(0)).toBeCloseTo(0, 4);
      expect(goc(1)).toBeCloseTo(Math.PI / 4, 4);
      expect(goc(2)).toBeCloseTo(Math.PI / 2, 4);
      expect(goc(4)).toBeCloseTo(Math.PI, 4);
    });

    it("nội suy đúng ở giữa MỌI cặp khung khoá, không chỉ cặp đầu", () => {
      // Đây là ca mà một lỗi lệch-một-đơn vị trong tìm nhị phân sẽ lộ ra: cặp
      // đầu vẫn đúng nhờ may, các cặp sau thì không.
      expect(goc(0.5)).toBeCloseTo(Math.PI / 8, 4);
      expect(goc(1.5)).toBeCloseTo((3 * Math.PI) / 8, 4);
      expect(goc(2.5)).toBeCloseTo((5 * Math.PI) / 8, 4);
      expect(goc(3.5)).toBeCloseTo((7 * Math.PI) / 8, 4);
    });

    it("track một khung khoá duy nhất không làm hỏng phép tìm", () => {
      const motKhung = sampleRetargetedClip({
        name: "idle",
        duration: 0,
        tracks: { leftUpperLeg: { times: [0], values: [...axisAngle("x", Math.PI / 3)] } },
      }, 12.5, true);
      expect(motKhung.leftUpperLeg?.[0]).toBeCloseTo(Math.PI / 3, 4);
    });
  });
});
