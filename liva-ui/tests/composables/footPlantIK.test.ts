import { describe, expect, it } from "vitest";
import { FootPlantIK } from "../../src/composables/footPlantIK";

describe("FootPlantIK", () => {
  it("eliminates horizontal (x, z) translation to prevent sawtooth wave pelvis stutter while avatar advances", () => {
    const ik = new FootPlantIK();
    const first = ik.update({
      state: "walk",
      leftFoot: { x: 0, y: 0, z: 0 },
      rightFoot: { x: 0.2, y: 0.08, z: 0 },
      delta: 1 / 60,
    });
    let moved = first;
    for (let frame = 0; frame < 20; frame++) {
      moved = ik.update({
        state: "walk",
        leftFoot: { x: 0.06, y: 0, z: 0 },
        rightFoot: { x: 0.25, y: 0.07, z: 0 },
        delta: 1 / 60,
      });
    }

    expect(first).toEqual({ x: 0, y: 0, z: 0 });
    // Horizontal translation is zeroed out to eliminate cyclic snapback
    expect(moved.x).toBeCloseTo(0, 5);
    expect(moved.y).toBeCloseTo(0, 5);
    expect(moved.z).toBeCloseTo(0, 5);
  });

  it("applies smooth vertical damping when foot height changes", () => {
    const ik = new FootPlantIK();
    // Anchor left foot at y = 0
    ik.update({
      state: "walk",
      leftFoot: { x: 0, y: 0, z: 0 },
      rightFoot: { x: 0.2, y: 0.08, z: 0 },
      delta: 1 / 60,
    });

    // Foot height changes vertically to y = 0.04 (stepping down relative to anchor)
    let moved = { x: 0, y: 0, z: 0 };
    for (let frame = 0; frame < 20; frame++) {
      moved = ik.update({
        state: "walk",
        leftFoot: { x: 0.05, y: 0.04, z: 0 },
        rightFoot: { x: 0.25, y: 0.08, z: 0 },
        delta: 1 / 60,
      });
    }

    expect(moved.x).toBe(0);
    expect(moved.z).toBe(0);
    // Vertical correction damps toward anchor.y - lockedPoint.y = -0.04
    expect(moved.y).toBeCloseTo(-0.04, 2);
  });

  it("switches the anchor to the other foot once the planted foot lifts", () => {
    const ik = new FootPlantIK();
    ik.update({
      state: "run",
      leftFoot: { x: 0, y: 0, z: 0 },
      rightFoot: { x: 0.2, y: 0.1, z: 0 },
      delta: 0.2,
    });
    const switched = ik.update({
      state: "run",
      leftFoot: { x: 0.08, y: 0.12, z: 0 },
      rightFoot: { x: 0.25, y: 0, z: 0 },
      delta: 1 / 60,
    });
    let after = switched;
    for (let frame = 0; frame < 20; frame++) {
      after = ik.update({
        state: "run",
        leftFoot: { x: 0.1, y: 0.14, z: 0 },
        rightFoot: { x: 0.29, y: 0, z: 0 },
        delta: 1 / 60,
      });
    }

    expect(switched).toEqual({ x: 0, y: 0, z: 0 });
    expect(after.x).toBe(0);
    expect(after.z).toBe(0);
  });

  it("hands off foot support smoothly without vertical snapping", () => {
    const ik = new FootPlantIK();
    ik.update({
      state: "run",
      leftFoot: { x: 0, y: 0, z: 0 },
      rightFoot: { x: 0.2, y: 0.1, z: 0 },
      delta: 0.1,
    });
    const beforeSwitch = ik.update({
      state: "run",
      leftFoot: { x: 0.1, y: 0.02, z: 0 },
      rightFoot: { x: 0.2, y: 0.1, z: 0 },
      delta: 0.1,
    });
    const switched = ik.update({
      state: "run",
      leftFoot: { x: 0.1, y: 0.12, z: 0 },
      rightFoot: { x: 0.2, y: 0, z: 0 },
      delta: 1 / 60,
    });

    expect(Math.abs(switched.y - beforeSwitch.y)).toBeLessThan(0.05);
    expect(switched.x).toBe(0);
    expect(switched.z).toBe(0);
  });

  it("releases the foot lock outside walk and run", () => {
    const ik = new FootPlantIK();
    ik.update({
      state: "walk",
      leftFoot: { x: 0, y: 0, z: 0 },
      rightFoot: { x: 0.2, y: 0.1, z: 0 },
      delta: 1 / 60,
    });

    expect(ik.update({
      state: "idle",
      leftFoot: { x: 0.1, y: 0, z: 0 },
      rightFoot: { x: 0.3, y: 0, z: 0 },
      delta: 1 / 60,
    })).toEqual({ x: 0, y: 0, z: 0 });
  });
});
