import type { LocomotionState } from "./useAvatarAnimation";

export interface Point3 {
  x: number;
  y: number;
  z: number;
}

export interface FootPlantFrame {
  state: LocomotionState;
  leftFoot: Point3;
  rightFoot: Point3;
  delta: number;
}

type FootSide = "left" | "right";

const ZERO = (): Point3 => ({ x: 0, y: 0, z: 0 });

export class FootPlantIK {
  private lockedSide: FootSide | null = null;
  private anchor: Point3 | null = null;
  private lockTime = 0;
  private currentCorrection: Point3 = ZERO();
  private readonly liftThreshold: number;
  private readonly minimumLockSeconds: number;
  private readonly maximumCorrection: number;
  private readonly correctionResponse: number;

  constructor(
    liftThreshold = 0.025,
    minimumLockSeconds = 0.12,
    maximumCorrection = 0.14,
    correctionResponse = 18,
  ) {
    this.liftThreshold = liftThreshold;
    this.minimumLockSeconds = minimumLockSeconds;
    this.maximumCorrection = maximumCorrection;
    this.correctionResponse = correctionResponse;
  }

  reset() {
    this.lockedSide = null;
    this.anchor = null;
    this.lockTime = 0;
    this.currentCorrection = ZERO();
  }

  private dampCorrection(target: Point3, delta: number): Point3 {
    const blend = 1 - Math.exp(-this.correctionResponse * Math.max(delta, 0));
    this.currentCorrection = {
      x: this.currentCorrection.x + (target.x - this.currentCorrection.x) * blend,
      y: this.currentCorrection.y + (target.y - this.currentCorrection.y) * blend,
      z: this.currentCorrection.z + (target.z - this.currentCorrection.z) * blend,
    };
    return { ...this.currentCorrection };
  }

  update(frame: FootPlantFrame): Point3 {
    if (frame.state !== "walk" && frame.state !== "run") {
      this.reset();
      return ZERO();
    }

    const candidate: FootSide = frame.leftFoot.y <= frame.rightFoot.y ? "left" : "right";
    const point = (side: FootSide) => side === "left" ? frame.leftFoot : frame.rightFoot;
    if (!this.lockedSide || !this.anchor) {
      this.lockedSide = candidate;
      this.anchor = { ...point(candidate) };
      this.lockTime = Math.max(frame.delta, 0);
      return this.dampCorrection(ZERO(), frame.delta);
    }

    this.lockTime += Math.max(frame.delta, 0);
    const lockedPoint = point(this.lockedSide);
    const candidatePoint = point(candidate);
    if (
      candidate !== this.lockedSide
      && this.lockTime >= this.minimumLockSeconds
      && lockedPoint.y - candidatePoint.y > this.liftThreshold
    ) {
      this.lockedSide = candidate;
      this.anchor = { ...candidatePoint };
      this.lockTime = 0;
      return this.dampCorrection(ZERO(), frame.delta);
    }

    const clamp = (value: number) => Math.min(Math.max(value, -this.maximumCorrection), this.maximumCorrection);
    return this.dampCorrection({
      x: clamp(this.anchor.x - lockedPoint.x),
      y: clamp(this.anchor.y - lockedPoint.y),
      z: clamp(this.anchor.z - lockedPoint.z),
    }, frame.delta);
  }
}
