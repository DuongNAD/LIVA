import { jest } from '@jest/globals';
(globalThis as any).jest = jest;

if (typeof (globalThis as any).expect?.extend === 'function') {
  (globalThis as any).expect.extend({
    toHaveBeenCalledOnce(received: any) {
      const pass = received && received.mock && received.mock.calls.length === 1;
      if (pass) {
        return {
          message: () => `expected mock not to have been called exactly once`,
          pass: true,
        };
      } else {
        const count = received && received.mock ? received.mock.calls.length : 0;
        return {
          message: () => `expected mock to have been called exactly once, but was called ${count} times`,
          pass: false,
        };
      }
    }
  });
}

const originalEnvs: Record<string, string | undefined> = {};

export const vi = {
  stubEnv: (name: string, value: string) => {
    if (!(name in originalEnvs)) {
      originalEnvs[name] = process.env[name];
    }
    process.env[name] = value;
  },
  unstubAllEnvs: () => {
    for (const name in originalEnvs) {
      const val = originalEnvs[name];
      if (val === undefined) {
        delete process.env[name];
      } else {
        process.env[name] = val;
      }
    }
    for (const name in originalEnvs) {
      delete originalEnvs[name];
    }
  },
  hoisted: <T>(fn: () => T): T => fn(),
  fn: (...args: any[]) => (jest as any).fn(...args),
  spyOn: (obj: any, prop: string, accessType?: string) => {
    if (obj === process && (prop === "platform" || prop === "arch")) {
      const originalValue = obj[prop];
      let mockValue = originalValue;
      const spy = {
        mockReturnValue: (val: any) => {
          mockValue = val;
          return spy;
        },
        mockRestore: () => {
          Object.defineProperty(obj, prop, {
            value: originalValue,
            configurable: true,
            writable: true
          });
        }
      };
      Object.defineProperty(obj, prop, {
        get: () => mockValue,
        configurable: true
      });
      return spy;
    }
    return (jest as any).spyOn(obj, prop, accessType);
  },
  mock: (path: string, factory?: any) => (jest as any).mock(path, factory),
  unmock: (path: string) => (jest as any).unmock(path),
  doMock: (path: string, factory?: any) => (jest as any).doMock(path, factory),
  doUnmock: (path: string) => (jest as any).unmock(path),
  clearAllMocks: () => (jest as any).clearAllMocks(),
  resetAllMocks: () => (jest as any).resetAllMocks(),
  restoreAllMocks: () => (jest as any).restoreAllMocks(),
  stubGlobal: (name: string, value: any) => {
    (globalThis as any)[name] = value;
  },
  unstubAllGlobals: () => {},
  mocked: <T>(obj: T) => (jest as any).mocked(obj as any),
  useFakeTimers: (options?: any) => {
    (jest as any).useFakeTimers({
      doNotFake: ['nextTick', 'queueMicrotask'],
      ...options
    });
  },
  useRealTimers: () => (jest as any).useRealTimers(),
  advanceTimersByTime: (ms: number) => (jest as any).advanceTimersByTime(ms),
  advanceTimersByTimeAsync: async (ms: number) => {
    await (jest as any).advanceTimersByTimeAsync(ms);
  },
  runAllTimers: () => {
    for (let i = 0; i < 10; i++) {
      (jest as any).runOnlyPendingTimers();
    }
  },
  runAllTimersAsync: async () => {
    for (let i = 0; i < 10; i++) {
      if (typeof (jest as any).runOnlyPendingTimersAsync === 'function') {
        await (jest as any).runOnlyPendingTimersAsync();
      } else {
        (jest as any).runOnlyPendingTimers();
      }
      for (let j = 0; j < 10; j++) {
        await Promise.resolve();
      }
    }
  },
  runOnlyPendingTimers: () => (jest as any).runOnlyPendingTimers(),
  runOnlyPendingTimersAsync: async () => {
    if (typeof (jest as any).runOnlyPendingTimersAsync === 'function') {
      await (jest as any).runOnlyPendingTimersAsync();
    } else {
      (jest as any).runOnlyPendingTimers();
    }
  },
  clearAllTimers: () => (jest as any).clearAllTimers(),
  getTimerCount: () => (jest as any).getTimerCount(),
  advanceTimersToNextTimerAsync: async () => {
    if (typeof (jest as any).advanceTimersToNextTimerAsync === 'function') {
      await (jest as any).advanceTimersToNextTimerAsync();
    } else {
      (jest as any).advanceTimersToNextTimer();
    }
  },
  advanceTimersToNextTimer: () => (jest as any).advanceTimersToNextTimer(),
  setSystemTime: (ms: number | Date) => (jest as any).setSystemTime(typeof ms === 'number' ? ms : ms.getTime()),
  resetModules: () => (jest as any).resetModules(),
  importActual: async (path: string) => (jest as any).requireActual(path),
};

export const describe = (globalThis as any).describe;
export const it = (globalThis as any).it;
export const test = (globalThis as any).test;
export const expect = (globalThis as any).expect;
export const beforeEach = (globalThis as any).beforeEach;
export const afterEach = (globalThis as any).afterEach;
export const beforeAll = (globalThis as any).beforeAll;
export const afterAll = (globalThis as any).afterAll;
export const suite = (globalThis as any).describe;

export const expectTypeOf = (...args: any[]) => ({
  toBeTypeOf: () => {},
  toEqualTypeOf: () => {},
  parameter: () => ({ toEqualTypeOf: () => {} }),
  returns: () => ({ toEqualTypeOf: () => {} }),
});

export type Reporter = any;
export type Task = any;
export type UserConsoleLog = any;
export type Mocked<T> = any;
