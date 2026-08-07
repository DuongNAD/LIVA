import { describe, it, expect } from "vitest";
import { useAvatarLocomotion } from "../../src/composables/useAvatarLocomotion";

/** Chạy vòng lặp với bước thời gian cố định, trả về ảnh chụp cuối cùng */
function run(loco: ReturnType<typeof useAvatarLocomotion>, seconds: number, step = 1 / 60) {
  let snap = loco.snapshot();
  for (let t = 0; t < seconds; t += step) snap = loco.update(step);
  return snap;
}

describe("useAvatarLocomotion", () => {
  it("bắt đầu ở chỗ được chỉ định và đứng yên", () => {
    const loco = useAvatarLocomotion({ start: { x: 0.85, y: 1 } });
    const snap = loco.snapshot();
    expect(snap).toMatchObject({ x: 0.85, y: 1, state: "idle", facing: 1, airborne: false });
  });

  it("đi tới đích rồi dừng lại đúng chỗ", () => {
    const loco = useAvatarLocomotion({ start: { x: 0.8, y: 1 } });
    loco.moveTo(0.2, 1);

    expect(loco.snapshot().state).toBe("walk"); // nhận lệnh là chuyển sang đi ngay
    const midway = loco.update(1 / 60);
    expect(midway.state).toBe("walk");
    expect(midway.facing).toBe(-1); // đi sang trái

    const arrived = run(loco, 12);
    expect(arrived.x).toBeCloseTo(0.2, 3);
    expect(arrived.state).toBe("idle");
  });

  it("chạy nhanh hơn đi bộ", () => {
    const walker = useAvatarLocomotion({ start: { x: 0.1, y: 1 } });
    const runner = useAvatarLocomotion({ start: { x: 0.1, y: 1 } });
    walker.moveTo(0.9, 1);
    runner.moveTo(0.9, 1, { run: true });

    const afterWalk = run(walker, 1);
    const afterRun = run(runner, 1);

    expect(afterRun.x).toBeGreaterThan(afterWalk.x);
    expect(afterWalk.state).toBe("walk");
    expect(afterRun.state).toBe("run");
  });

  it("tăng tốc dần khi bắt đầu thay vì nhảy ngay lên vận tốc tối đa", () => {
    const loco = useAvatarLocomotion({ start: { x: 0.1, y: 1 } });
    loco.moveTo(0.9, 1);

    const start = loco.snapshot();
    const first = loco.update(1 / 60);
    const second = loco.update(1 / 60);
    const firstStep = first.x - start.x;
    const secondStep = second.x - first.x;

    expect(first.speed).toBeGreaterThan(0);
    expect(second.speed).toBeGreaterThan(first.speed);
    expect(secondStep).toBeGreaterThan(firstStep);
    expect(first.motion).toBeGreaterThan(0);
    expect(first.motion).toBeLessThan(1);
  });

  it("phanh dần trước đích thay vì chạy hết tốc độ rồi dừng giật", () => {
    const loco = useAvatarLocomotion({ start: { x: 0.1, y: 1 } });
    loco.moveTo(0.45, 1);
    const movingSpeeds: number[] = [];

    for (let i = 0; i < 600; i++) {
      const snap = loco.update(1 / 60);
      if (snap.state === "idle") break;
      movingSpeeds.push(snap.speed);
    }

    const peak = Math.max(...movingSpeeds);
    expect(peak).toBeGreaterThan(0.08);
    expect(movingSpeeds.at(-1)).toBeLessThan(peak * 0.5);
    expect(loco.snapshot().x).toBeCloseTo(0.45, 3);
    expect(loco.snapshot().motion).toBe(0);
  });

  it("kẹp đích trong biên để nhân vật không ra khỏi màn hình", () => {
    const loco = useAvatarLocomotion({
      start: { x: 0.5, y: 1 },
      bounds: { minX: 0.1, maxX: 0.9, minY: 0.6, maxY: 1 },
    });
    loco.moveTo(5, -3);
    const snap = run(loco, 20);
    expect(snap.x).toBeCloseTo(0.9, 3);
    expect(snap.y).toBeCloseTo(0.6, 3);
  });

  it("nhảy theo đường parabol rồi tiếp đất đúng cao độ cũ", () => {
    const loco = useAvatarLocomotion({ start: { x: 0.5, y: 1 } });
    loco.jump();

    // 30 tick ≈ 0,5 s — vẫn còn trong quãng bay 0,62 s
    const heights: number[] = [];
    for (let i = 0; i < 30; i++) heights.push(loco.update(1 / 60).y);

    const peak = Math.min(...heights); // y nhỏ = cao hơn trên màn hình
    expect(peak).toBeLessThan(1); // có rời mặt đất
    expect(loco.snapshot().state).toBe("jump");
    expect(loco.snapshot().airborne).toBe(true);

    const landed = run(loco, 1);
    expect(landed.y).toBeCloseTo(1, 5); // về đúng mặt đất, không trôi
    expect(landed.airborne).toBe(false);
    expect(landed.state).toBe("idle");
  });

  it("không cho nhảy chồng khi đang ở trên không", () => {
    const loco = useAvatarLocomotion();
    loco.jump();
    loco.update(0.3);
    const midAir = loco.snapshot().y;
    loco.jump(); // phải bị bỏ qua
    loco.update(0.0001);
    expect(loco.snapshot().y).toBeCloseTo(midAir, 2);
  });

  it("teleport dời chỗ tức thì và huỷ chặng đang đi", () => {
    const loco = useAvatarLocomotion({ start: { x: 0.5, y: 1 } });
    loco.moveTo(0.1, 1);
    loco.update(0.2);
    loco.teleport(0.75, 0.8);

    const snap = loco.snapshot();
    expect(snap.x).toBeCloseTo(0.75, 5);
    expect(snap.y).toBeCloseTo(0.8, 5);
    expect(snap.state).toBe("idle");

    const later = run(loco, 2);
    expect(later.x).toBeCloseTo(0.75, 5); // không đi tiếp về đích cũ
  });

  it("tự chọn đích mới khi bật đi lang thang", () => {
    // Nguồn ngẫu nhiên tất định để kết quả lặp lại được
    let seed = 0;
    const random = () => {
      seed = (seed * 9301 + 49297) % 233280;
      return seed / 233280;
    };
    const loco = useAvatarLocomotion({ start: { x: 0.5, y: 1 }, random });

    const startX = loco.snapshot().x;
    run(loco, 20);
    expect(loco.snapshot().state).toBe("idle"); // chưa bật thì đứng yên mãi
    expect(loco.snapshot().x).toBe(startX);

    loco.setWander(true);
    expect(loco.isWandering()).toBe(true);

    // Đi lang thang xen kẽ nghỉ, nên kiểm tra "có lúc di chuyển" trong một
    // quãng đủ dài, thay vì bắt đúng khoảnh khắc đang bước.
    let sawMovement = false;
    for (let i = 0; i < 60 * 40; i++) {
      if (loco.update(1 / 60).state !== "idle") sawMovement = true;
    }
    expect(sawMovement).toBe(true);
    expect(loco.snapshot().x).not.toBe(startX);

    loco.setWander(false);
    loco.stop();
    const stopped = run(loco, 30);
    expect(stopped.state).toBe("idle");
  });

  it("bỏ qua tick có delta bằng 0 mà không nhích vị trí", () => {
    const loco = useAvatarLocomotion({ start: { x: 0.4, y: 1 } });
    loco.moveTo(0.9, 1);
    const before = loco.snapshot().x;
    loco.update(0);
    expect(loco.snapshot().x).toBe(before);
  });
});
