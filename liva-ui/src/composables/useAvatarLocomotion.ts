/**
 * useAvatarLocomotion.ts — LIVA đi lại trên màn hình
 * ===================================================
 * Máy trạng thái thuần tuý: chỉ tính TOẠ ĐỘ và TRẠNG THÁI, không đụng THREE.js.
 * Tách bạch như vậy vì hai việc khác hẳn nhau — "đi từ trái sang phải màn hình"
 * là dịch chuyển gốc nhân vật, còn "trông như đang bước" là xoay xương; gộp lại
 * thì không thứ nào kiểm thử được tử tế.
 *
 * Mọi toạ độ đều chuẩn hoá [0,1] theo khung nhìn: (0,0) góc trái trên,
 * y tính theo CHÂN nhân vật. Khớp thẳng với setScreenPosition() của use3DModel.
 */
import type { LocomotionState } from "./useAvatarAnimation";

export interface LocomotionSnapshot {
  /** Vị trí chân, đã gồm độ cao khi đang nhảy */
  x: number;
  y: number;
  state: LocomotionState;
  /** 1 = quay sang phải màn hình, -1 = sang trái */
  facing: 1 | -1;
  /** Đang ở trên không hay không — vòng lặp render dùng để khoá tư thế nhảy */
  airborne: boolean;
  /** Vận tốc tịnh tiến hiện tại, theo phần màn hình mỗi giây. */
  speed: number;
  /** Cường độ chuyển động chuẩn hoá [0,1], dùng để đồng bộ nhịp xương. */
  motion: number;
}

export interface MoveOptions {
  /** Chạy thay vì đi bộ */
  run?: boolean;
}

export interface LocomotionOptions {
  /** Vị trí xuất phát */
  start?: { x: number; y: number };
  /** Biên di chuyển, mặc định chừa mép để nhân vật không dính cạnh màn hình */
  bounds?: { minX: number; maxX: number; minY: number; maxY: number };
  /** Nguồn ngẫu nhiên — thay được để kiểm thử tất định */
  random?: () => number;
}

export interface AvatarLocomotionApi {
  snapshot: () => LocomotionSnapshot;
  moveTo: (x: number, y: number, options?: MoveOptions) => void;
  /** Đặt vị trí tức thì, huỷ mọi chặng đang đi. Dùng khi bên ngoài ấn định chỗ đứng. */
  teleport: (x: number, y: number) => void;
  jump: () => void;
  stop: () => void;
  /** Bật/tắt tự đi lang thang khi rảnh */
  setWander: (enabled: boolean) => void;
  isWandering: () => boolean;
  update: (delta: number) => LocomotionSnapshot;
  reset: () => void;
}

/** Tốc độ tính theo phần chiều rộng màn hình mỗi giây */
const WALK_SPEED = 0.085;
const RUN_SPEED = 0.24;
const WALK_ACCELERATION = 0.28;
const RUN_ACCELERATION = 0.75;
const WALK_DECELERATION = 0.34;
const RUN_DECELERATION = 0.9;
/** Coi như đã tới nơi khi còn cách dưới ngưỡng này */
const ARRIVE_EPSILON = 0.004;

const JUMP_SECONDS = 0.62;
/** Độ cao đỉnh nhảy, theo phần chiều cao màn hình */
const JUMP_HEIGHT = 0.13;

/** Khoảng nghỉ giữa hai chặng lang thang (giây) */
const WANDER_PAUSE_MIN = 2.5;
const WANDER_PAUSE_MAX = 7;

const DEFAULT_BOUNDS = { minX: 0.06, maxX: 0.94, minY: 0.55, maxY: 1 };

function clamp(value: number, min: number, max: number): number {
  return Math.min(Math.max(value, min), max);
}

export function useAvatarLocomotion(options: LocomotionOptions = {}): AvatarLocomotionApi {
  const bounds = { ...DEFAULT_BOUNDS, ...options.bounds };
  const random = options.random ?? Math.random;
  const startX = options.start?.x ?? 0.85;
  const startY = options.start?.y ?? 1;

  let x = startX;
  let groundY = startY;
  let facing: 1 | -1 = 1;

  let targetX: number | null = null;
  let targetY: number | null = null;
  let speed = WALK_SPEED;
  let currentSpeed = 0;
  let running = false;

  let jumpTime: number | null = null;

  let wander = false;
  let wanderPause = 0;

  function currentY(): number {
    if (jumpTime === null) return groundY;
    // Parabol: 4·p·(1−p) đạt đỉnh 1 ở giữa quãng, về 0 ở hai đầu
    const p = jumpTime / JUMP_SECONDS;
    return groundY - JUMP_HEIGHT * 4 * p * (1 - p);
  }

  function currentState(): LocomotionState {
    if (jumpTime !== null) return "jump";
    if (targetX === null && targetY === null) return "idle";
    return running ? "run" : "walk";
  }

  function snapshot(): LocomotionSnapshot {
    return {
      x,
      y: currentY(),
      state: currentState(),
      facing,
      airborne: jumpTime !== null,
      speed: currentSpeed,
      motion: targetX === null && targetY === null ? 0 : clamp(currentSpeed / speed, 0, 1),
    };
  }

  function moveTo(nextX: number, nextY: number, moveOptions: MoveOptions = {}) {
    targetX = clamp(nextX, bounds.minX, bounds.maxX);
    targetY = clamp(nextY, bounds.minY, bounds.maxY);
    running = moveOptions.run === true;
    speed = running ? RUN_SPEED : WALK_SPEED;
    if (targetX !== x) facing = targetX > x ? 1 : -1;
  }

  function teleport(nextX: number, nextY: number) {
    x = clamp(nextX, bounds.minX, bounds.maxX);
    groundY = clamp(nextY, bounds.minY, bounds.maxY);
    targetX = null;
    targetY = null;
    running = false;
    currentSpeed = 0;
    jumpTime = null;
  }

  function jump() {
    if (jumpTime !== null) return; // đang bay thì không nhảy chồng
    jumpTime = 0;
  }

  function stop() {
    targetX = null;
    targetY = null;
    running = false;
    currentSpeed = 0;
  }

  function pickWanderTarget() {
    const nextX = bounds.minX + random() * (bounds.maxX - bounds.minX);
    // Phần lớn thời gian đứng dưới đất; thỉnh thoảng mới lên cao
    const nextY = random() < 0.75 ? bounds.maxY : bounds.minY + random() * (bounds.maxY - bounds.minY);
    moveTo(nextX, nextY, { run: random() < 0.25 });
  }

  function update(delta: number): LocomotionSnapshot {
    if (delta > 0 && jumpTime !== null) {
      jumpTime += delta;
      if (jumpTime >= JUMP_SECONDS) jumpTime = null;
    }

    if (delta > 0 && (targetX !== null || targetY !== null)) {
      const goalX = targetX ?? x;
      const goalY = targetY ?? groundY;
      const dx = goalX - x;
      const dy = goalY - groundY;
      const distance = Math.hypot(dx, dy);

      if (distance <= ARRIVE_EPSILON) {
        x = goalX;
        groundY = goalY;
        stop();
      } else {
        const acceleration = running ? RUN_ACCELERATION : WALK_ACCELERATION;
        const deceleration = running ? RUN_DECELERATION : WALK_DECELERATION;
        const brakingSpeed = Math.sqrt(2 * deceleration * Math.max(distance - ARRIVE_EPSILON, 0));
        const desiredSpeed = Math.min(speed, brakingSpeed);
        if (currentSpeed < desiredSpeed) {
          currentSpeed = Math.min(desiredSpeed, currentSpeed + acceleration * delta);
        } else {
          currentSpeed = Math.max(desiredSpeed, currentSpeed - deceleration * delta);
        }
        const step = Math.min(currentSpeed * delta, distance);
        x += (dx / distance) * step;
        groundY += (dy / distance) * step;
      }
    }

    if (wander && targetX === null && targetY === null && jumpTime === null) {
      wanderPause -= delta;
      if (wanderPause <= 0) {
        pickWanderTarget();
        wanderPause = WANDER_PAUSE_MIN + random() * (WANDER_PAUSE_MAX - WANDER_PAUSE_MIN);
      }
    }

    return snapshot();
  }

  function reset() {
    x = startX;
    groundY = startY;
    facing = 1;
    targetX = null;
    targetY = null;
    running = false;
    currentSpeed = 0;
    jumpTime = null;
    wander = false;
    wanderPause = 0;
  }

  return {
    snapshot,
    moveTo,
    teleport,
    jump,
    stop,
    setWander: (enabled: boolean) => {
      wander = enabled;
      if (enabled && wanderPause <= 0) wanderPause = WANDER_PAUSE_MIN;
    },
    isWandering: () => wander,
    update,
    reset,
  };
}
