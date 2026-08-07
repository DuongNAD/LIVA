import { describe, expect, it } from "vitest";
import { FootPlantIK } from "../../src/composables/footPlantIK";

describe("FootPlantIK", () => {
  it("keeps the planted foot at its world anchor while the avatar root advances", () => {
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
    expect(moved.x).toBeCloseTo(-0.06, 3);
    expect(moved.y).toBeCloseTo(0, 5);
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
    expect(after.x).toBeCloseTo(-0.04, 3);
  });

  it("hands off foot support without snapping the pelvis correction to zero", () => {
    const ik = new FootPlantIK();
    ik.update({
      state: "run",
      leftFoot: { x: 0, y: 0, z: 0 },
      rightFoot: { x: 0.2, y: 0.1, z: 0 },
      delta: 0.1,
    });
    const beforeSwitch = ik.update({
      state: "run",
      leftFoot: { x: 0.1, y: 0, z: 0 },
      rightFoot: { x: 0.2, y: 0.1, z: 0 },
      delta: 0.1,
    });
    const switched = ik.update({
      state: "run",
      leftFoot: { x: 0.1, y: 0.12, z: 0 },
      rightFoot: { x: 0.2, y: 0, z: 0 },
      delta: 1 / 60,
    });

    expect(Math.abs(switched.x - beforeSwitch.x)).toBeLessThan(0.03);
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
