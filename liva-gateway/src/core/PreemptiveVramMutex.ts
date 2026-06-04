import { EventEmitter } from 'events';
import { logger } from '../utils/logger';

export type AvatarDemoteLevel = 'normal' | 'eco' | 'freeze' | 'preempted';

export interface AvatarDemotePayload {
  readonly level: 'eco' | 'freeze';
  readonly fps: number;
}

export interface AvatarRestorePayload {
  readonly level: 'normal';
}

export interface VramLockHandle {
  readonly id: string;
  readonly requiredMemory: number;
  readonly priority: number;
  readonly signal: AbortSignal;
  release: () => void;
}

export interface VramRequest {
  id: string;
  requiredMemory: number;
  priority: number;
  enqueuedAt: number;
  resolve: (handle: VramLockHandle) => void;
  reject: (error: Error) => void;
}

interface ActiveLock {
  id: string;
  requiredMemory: number;
  priority: number;
  controller: AbortController;
  acquiredAt: number;
}

export class PreemptiveVramMutex {
  private totalVram: number;
  private allocatedVram: number;
  private queue: VramRequest[];
  private activeLocks: ActiveLock[];
  public eventEmitter: EventEmitter;
  private isProcessing: boolean;

  // Graduated degradation state
  private avatarDemoteLevel: AvatarDemoteLevel = 'normal';
  private demotionTriggeredBy: string | null = null;

  // Circuit Breaker State
  private failureCount: number = 0;
  private isCircuitOpen: boolean = false;
  private readonly MAX_FAILURES = 3;
  private readonly CIRCUIT_COOLOFF_MS = 10000;

  constructor(totalVramMemoryMb: number) {
    this.totalVram = totalVramMemoryMb;
    this.allocatedVram = 0;
    this.queue = [];
    this.activeLocks = [];
    this.eventEmitter = new EventEmitter();
    this.isProcessing = false;
  }

  public async acquire(
    id: string,
    requiredMemory: number,
    priority: number = 0,
    timeoutMs: number = 10000
  ): Promise<VramLockHandle> {
    if (this.isCircuitOpen) {
      throw new Error(`[VramMutex] CIRCUIT OPEN. VRAM is unstable. Rejecting task: ${id}`);
    }
    return new Promise<VramLockHandle>((resolve, reject) => {
      const request: VramRequest = { id, requiredMemory, priority, enqueuedAt: Date.now(), resolve, reject };
      this.queue.push(request);
      
      // Sort by priority descending (higher number = higher priority),
      // and FIFO for same priority.
      this.queue.sort((a, b) => b.priority - a.priority || this.queue.indexOf(a) - this.queue.indexOf(b));

      const timeout = setTimeout(() => {
        const index = this.queue.findIndex(req => req.id === id);
        if (index !== -1) {
          this.queue.splice(index, 1);
          reject(new Error(`VRAM Acquisition Timeout: Yêu cầu ${id} bị từ chối sau ${timeoutMs}ms do vượt quá giới hạn hạn định.`));
        }
      }, timeoutMs);

      const originalResolve = resolve;
      request.resolve = (handle) => {
        clearTimeout(timeout);
        originalResolve(handle);
      };

      this.processQueue();
    });
  }

  public release(requiredMemory: number, lockId?: string): void {
    if (lockId) {
      const index = this.activeLocks.findIndex(lock => lock.id === lockId);
      if (index !== -1) {
        const lock = this.activeLocks[index];
        this.activeLocks.splice(index, 1);
        this.allocatedVram = Math.max(0, this.allocatedVram - lock.requiredMemory);

        const duration = Date.now() - lock.acquiredAt;
        this.eventEmitter.emit('vram_lock_hold_duration', { id: lock.id, durationMs: duration, preempted: false });
      }

      // Auto-restore avatar if the releasing lock triggered demotion
      if (this.demotionTriggeredBy === lockId && this.avatarDemoteLevel !== 'normal') {
        this.restoreAvatar();
      }
    } else {
      this.allocatedVram = Math.max(0, this.allocatedVram - requiredMemory);
    }
    this.eventEmitter.emit('vram_released');
    this.processQueue();
  }

  /**
   * Graduated VRAM degradation: Attempts eco-mode and freeze before hard preemption.
   * Prevents jarring avatar disappearance by giving the UI time to free VRAM gracefully.
   *
   * Step 1: Eco Mode — throttle avatar to 5fps, wait 500ms
   * Step 2: Freeze Mode — pause avatar rendering entirely, wait 500ms
   * Step 3: Hard Preempt — fall through to standard preemptive acquire
   */
  public async acquireWithGraduation(
    id: string,
    requiredMemory: number,
    priority: number = 0,
    timeoutMs: number = 10000
  ): Promise<VramLockHandle> {
    const availableVram = this.totalVram - this.allocatedVram;

    // If enough VRAM is available, skip graduation entirely
    if (availableVram >= requiredMemory) {
      return this.acquire(id, requiredMemory, priority, timeoutMs);
    }

    // Only attempt graduation for high-priority tasks that would trigger preemption
    if (priority < 10) {
      return this.acquire(id, requiredMemory, priority, timeoutMs);
    }

    const STEP_WAIT_MS = 500;

    // Step 1: Eco Mode — throttle avatar to 5fps
    logger.info(`[VramMutex] Graduated degradation Step 1 (eco) for task: ${id}`);
    this.setAvatarDemoteLevel('eco', id);
    this.eventEmitter.emit('avatar_demote', { level: 'eco', fps: 5 } satisfies AvatarDemotePayload);
    await this.delay(STEP_WAIT_MS);

    // Check if eco-mode freed enough VRAM
    const afterEcoVram = this.totalVram - this.allocatedVram;
    if (afterEcoVram >= requiredMemory) {
      logger.info(`[VramMutex] Eco mode freed sufficient VRAM for task: ${id}`);
      const handle = await this.acquire(id, requiredMemory, priority, timeoutMs);
      return this.wrapHandleWithRestore(handle, id);
    }

    // Step 2: Freeze Mode — pause avatar rendering completely
    logger.info(`[VramMutex] Graduated degradation Step 2 (freeze) for task: ${id}`);
    this.setAvatarDemoteLevel('freeze', id);
    this.eventEmitter.emit('avatar_demote', { level: 'freeze', fps: 0 } satisfies AvatarDemotePayload);
    await this.delay(STEP_WAIT_MS);

    // Check if freeze freed enough VRAM
    const afterFreezeVram = this.totalVram - this.allocatedVram;
    if (afterFreezeVram >= requiredMemory) {
      logger.info(`[VramMutex] Freeze mode freed sufficient VRAM for task: ${id}`);
      const handle = await this.acquire(id, requiredMemory, priority, timeoutMs);
      return this.wrapHandleWithRestore(handle, id);
    }

    // Step 3: Hard preempt — fall through to standard preemptive acquire
    logger.warn(`[VramMutex] Graduated degradation exhausted. Hard preempt for task: ${id}`);
    this.setAvatarDemoteLevel('preempted', id);
    const handle = await this.acquire(id, requiredMemory, priority, timeoutMs);
    return this.wrapHandleWithRestore(handle, id);
  }

  public getAvatarDemoteLevel(): AvatarDemoteLevel {
    return this.avatarDemoteLevel;
  }

  /**
   * Sets the current avatar demotion level and tracks which lock triggered it.
   */
  private setAvatarDemoteLevel(level: AvatarDemoteLevel, triggeredBy: string): void {
    this.avatarDemoteLevel = level;
    this.demotionTriggeredBy = triggeredBy;
  }

  /**
   * Restores avatar to normal state and emits the restore event.
   */
  private restoreAvatar(): void {
    logger.info(`[VramMutex] Restoring avatar from '${this.avatarDemoteLevel}' to normal.`);
    this.avatarDemoteLevel = 'normal';
    this.demotionTriggeredBy = null;
    this.eventEmitter.emit('avatar_restore', { level: 'normal' } satisfies AvatarRestorePayload);
  }

  /**
   * Wraps a VramLockHandle so that releasing it also triggers avatar restoration.
   */
  private wrapHandleWithRestore(handle: VramLockHandle, triggeringLockId: string): VramLockHandle {
    const originalRelease = handle.release;
    return {
      ...handle,
      release: () => {
        originalRelease();
        if (this.demotionTriggeredBy === triggeringLockId && this.avatarDemoteLevel !== 'normal') {
          this.restoreAvatar();
        }
      }
    };
  }

  /**
   * Promise-based delay utility for graduated degradation steps.
   */
  private delay(ms: number): Promise<void> {
    return new Promise<void>(resolve => setTimeout(resolve, ms));
  }

  private async processQueue(): Promise<void> {
    if (this.isProcessing) return;
    this.isProcessing = true;

    try {
      while (this.queue.length > 0) {
        const nextRequest = this.queue[0];
        const availableVram = this.totalVram - this.allocatedVram;

        if (availableVram >= nextRequest.requiredMemory) {
          this.queue.shift();
          this.allocatedVram += nextRequest.requiredMemory;
          
          const controller = new AbortController();
          const activeLock: ActiveLock = {
            id: nextRequest.id,
            requiredMemory: nextRequest.requiredMemory,
            priority: nextRequest.priority,
            controller,
            acquiredAt: Date.now()
          };
          this.activeLocks.push(activeLock);

          const waitLatency = Date.now() - nextRequest.enqueuedAt;
          this.eventEmitter.emit('vram_wait_latency', { id: nextRequest.id, latencyMs: waitLatency });

          const handle: VramLockHandle = {
            id: nextRequest.id,
            requiredMemory: nextRequest.requiredMemory,
            priority: nextRequest.priority,
            signal: controller.signal,
            release: () => {
              this.release(nextRequest.requiredMemory, nextRequest.id);
            }
          };

          nextRequest.resolve(handle);
        } else {
          // Preemption logic:
          // If next request is high priority (>= 10) and VRAM is full,
          // we can preempt any low-priority running task (< 10).
          if (nextRequest.priority >= 10) {
            const lowPriorityLocks = this.activeLocks.filter(lock => lock.priority < 10);
            
            if (lowPriorityLocks.length > 0) {
              // Sort by priority ascending (preempt lowest first)
              lowPriorityLocks.sort((a, b) => a.priority - b.priority);
              const victim = lowPriorityLocks[0];
              
              // Abort and release immediately
              victim.controller.abort(`Preempted by higher priority task: ${nextRequest.id}`);
              
              const victimIndex = this.activeLocks.findIndex(lock => lock.id === victim.id);
              if (victimIndex !== -1) {
                this.activeLocks.splice(victimIndex, 1);
                this.allocatedVram = Math.max(0, this.allocatedVram - victim.requiredMemory);

                const duration = Date.now() - victim.acquiredAt;
                this.eventEmitter.emit('vram_lock_hold_duration', { id: victim.id, durationMs: duration, preempted: true });
              }
              
              this.eventEmitter.emit('vram_released');
              continue;
            }
          }
          break;
        }
      }
    } finally {
      this.isProcessing = false;
    }
  }

  /**
   * Tối ưu VRAM: Bao bọc thực thi an toàn.
   * Ngăn chặn hoàn toàn tình trạng treo VRAM do lỗi Runtime.
   */
  private triggerCircuitBreaker(): void {
    logger.error('[VramMutex] CRITICAL: Circuit Breaker TRIPPED. Triggering VRAM Reset sequence.');
    this.isCircuitOpen = true;
    this.eventEmitter.emit('emergency_reset_required');
    
    // Clear queue, reject all pending
    while (this.queue.length > 0) {
      const task = this.queue.shift();
      task?.reject(new Error('[VramMutex] Task aborted due to VRAM Circuit Breaker trip.'));
    }

    // Auto-recovery sau khoảng thời gian cool-off
    setTimeout(() => {
      logger.warn('[VramMutex] Circuit Breaker HALF-OPEN. Resuming normal operations.');
      this.isCircuitOpen = false;
      this.failureCount = 0;
    }, this.CIRCUIT_COOLOFF_MS);
  }

  /**
   * Tối ưu VRAM: Bao bọc thực thi an toàn.
   * Ngăn chặn hoàn toàn tình trạng treo VRAM do lỗi Runtime.
   * Đồng thời kích hoạt Circuit Breaker nếu có quá nhiều lỗi liên tiếp.
   */
  public async executeSafely<T>(
    id: string,
    requiredMemory: number,
    task: () => Promise<T>,
    priority: number = 0,
    timeoutMs: number = 10000
  ): Promise<T> {
    const handle = await this.acquire(id, requiredMemory, priority, timeoutMs);
    try {
      const result = await task();
      // Reset bộ đếm lỗi nếu thành công
      this.failureCount = 0;
      return result;
    } catch (error) {
      logger.error({ err: error }, `[VramMutex] Task ${id} crashed.`);
      this.failureCount++;
      if (this.failureCount >= this.MAX_FAILURES && !this.isCircuitOpen) {
        this.triggerCircuitBreaker();
      }
      throw error;
    } finally {
      handle.release();
    }
  }

  public getStatus() {
    return {
      totalVram: this.totalVram,
      allocatedVram: this.allocatedVram,
      availableVram: this.totalVram - this.allocatedVram,
      pendingRequests: this.queue.length
    };
  }
}
