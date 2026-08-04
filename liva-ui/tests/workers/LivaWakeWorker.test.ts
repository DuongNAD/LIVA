import { vi, describe, it, expect, beforeEach, afterEach } from 'vitest';

/**
 * Worker này KHÔNG còn nhận dạng wake word — nó cắt câu để core xác minh.
 * Nên phần đáng test là biên câu: cụm ngắn hợp lệ thì phát ra, tiếng động chớp
 * nhoáng và im lặng thì không.
 */

/** Đúng bằng frameSize của mic-capture.worklet: 512 mẫu = 32 ms @ 16 kHz. */
const FRAME = 512;

function speechFrame(amplitude = 0.1): Float32Array {
  const f = new Float32Array(FRAME);
  for (let i = 0; i < FRAME; i++) {
    // Sin 200 Hz — RMS = amplitude/√2 ≈ 0.07, thừa trên sàn mặc định 0.015.
    f[i] = amplitude * Math.sin((2 * Math.PI * 200 * i) / 16000);
  }
  return f;
}

function silentFrame(): Float32Array {
  return new Float32Array(FRAME);
}

describe('LivaWakeWorker', () => {
  let originalSelf: any;
  let postMessageMock: any;
  let closeMock: any;

  const feed = async (onmessage: any, frames: Float32Array[]) => {
    for (const frame of frames) {
      // Sao chép: worker nhận quyền sở hữu buffer trong môi trường thật.
      await onmessage({ data: { type: 'audio', data: { audio: new Float32Array(frame).buffer } } });
    }
  };

  const candidates = () =>
    postMessageMock.mock.calls
      .map((call: any[]) => call[0])
      .filter((msg: any) => msg?.type === 'candidate');

  beforeEach(() => {
    vi.resetModules();
    originalSelf = globalThis.self;

    postMessageMock = vi.fn();
    closeMock = vi.fn();

    globalThis.self = {
      postMessage: postMessageMock,
      close: closeMock,
    } as any;
  });

  afterEach(() => {
    globalThis.self = originalSelf;
  });

  it('báo loaded rồi ready sau init', async () => {
    await import('../../src/workers/LivaWakeWorker');

    expect(postMessageMock).toHaveBeenCalledWith({ type: 'loaded' });

    const onmessage = (globalThis.self as any).onmessage;
    expect(onmessage).toBeDefined();

    await onmessage({ data: { type: 'init', data: { config: { speechFloor: 0.02 } } } });

    expect(postMessageMock).toHaveBeenCalledWith(
      expect.objectContaining({ type: 'ready', success: true })
    );
  });

  it('im lặng thuần thì không bao giờ phát cụm ứng viên', async () => {
    await import('../../src/workers/LivaWakeWorker');
    const onmessage = (globalThis.self as any).onmessage;
    await onmessage({ data: { type: 'init' } });

    await feed(onmessage, Array.from({ length: 60 }, silentFrame));

    expect(candidates()).toHaveLength(0);
  });

  it('cụm nói ~640 ms rồi im thì phát ra đúng một ứng viên', async () => {
    await import('../../src/workers/LivaWakeWorker');
    const onmessage = (globalThis.self as any).onmessage;
    await onmessage({ data: { type: 'init' } });

    await feed(onmessage, [
      ...Array.from({ length: 20 }, () => speechFrame()), // 640 ms
      ...Array.from({ length: 8 }, silentFrame), // đủ hangover
    ]);

    const found = candidates();
    expect(found).toHaveLength(1);
    // 20 khung × 32 ms, biên đặt ở độ phân giải khung nên cho phép ±1 khung.
    expect(found[0].speechMs).toBeGreaterThan(600);
    expect(found[0].speechMs).toBeLessThan(680);
    // Audio gửi đi phải gồm cả pre-roll, tức dài hơn chính đoạn nói.
    expect(found[0].audio.byteLength / 4).toBeGreaterThan((640 / 1000) * 16000);
  });

  it('tiếng động chớp nhoáng dưới minUtteranceMs bị loại', async () => {
    await import('../../src/workers/LivaWakeWorker');
    const onmessage = (globalThis.self as any).onmessage;
    await onmessage({ data: { type: 'init' } });

    await feed(onmessage, [
      ...Array.from({ length: 3 }, () => speechFrame()), // ~96 ms
      ...Array.from({ length: 8 }, silentFrame),
    ]);

    expect(candidates()).toHaveLength(0);
  });

  it('đoạn giọng quá ngắn 320 ms không được gửi sang STT', async () => {
    await import('../../src/workers/LivaWakeWorker');
    const onmessage = (globalThis.self as any).onmessage;
    await onmessage({ data: { type: 'init' } });

    await feed(onmessage, [
      ...Array.from({ length: 10 }, () => speechFrame()), // 320 ms
      ...Array.from({ length: 8 }, silentFrame),
    ]);

    expect(candidates()).toHaveLength(0);
  });

  it('cắt phần mở đầu khi người dùng nói một mạch quá dài', async () => {
    await import('../../src/workers/LivaWakeWorker');
    const onmessage = (globalThis.self as any).onmessage;
    // maxUtteranceMs nhỏ để test nhanh. Phải hạ minProbeMs theo, nếu không sàn
    // độ dài tối thiểu sẽ nới ngược lại đúng phần vừa cắt.
    await onmessage({
      data: { type: 'init', data: { config: { maxUtteranceMs: 500, minProbeMs: 0 } } },
    });

    await feed(onmessage, [
      ...Array.from({ length: 60 }, () => speechFrame()), // ~1,9 s liên tục
      ...Array.from({ length: 8 }, silentFrame),
    ]);

    const found = candidates();
    expect(found).toHaveLength(1);
    const sentSamples = found[0].audio.byteLength / 4;
    const cap = FRAME * 8 + (500 / 1000) * 16000; // preroll + maxUtterance
    expect(sentSamples).toBeLessThanOrEqual(cap);
  });

  /**
   * Sàn độ dài là ràng buộc CỨNG từ phía core, không phải tinh chỉnh: classifier
   * cần ~1,96 s mới chạy, STT cần ≳1,3 s mới ra chữ. Một cụm "hey Liva" chỉ dài
   * ~0,7 s nên nếu không nới ngược thì cả hai tầng đều im lặng.
   */
  it('nới ngược vào vòng đệm cho đủ minProbeMs', async () => {
    await import('../../src/workers/LivaWakeWorker');
    const onmessage = (globalThis.self as any).onmessage;
    await onmessage({ data: { type: 'init' } });

    await feed(onmessage, [
      ...Array.from({ length: 90 }, silentFrame), // ~2,9 s nền phòng có sẵn
      ...Array.from({ length: 22 }, () => speechFrame()), // ~0,7 s "hey Liva"
      ...Array.from({ length: 8 }, silentFrame),
    ]);

    const found = candidates();
    expect(found).toHaveLength(1);
    const sentMs = (found[0].audio.byteLength / 4 / 16000) * 1000;
    expect(sentMs).toBeGreaterThanOrEqual(2300);
  });

  it('cooldown chặn cụm thứ hai đến quá sớm', async () => {
    await import('../../src/workers/LivaWakeWorker');
    const onmessage = (globalThis.self as any).onmessage;
    await onmessage({ data: { type: 'init' } });

    const utterance = [
      ...Array.from({ length: 20 }, () => speechFrame()),
      ...Array.from({ length: 8 }, silentFrame),
    ];
    await feed(onmessage, utterance);
    await feed(onmessage, utterance); // ngay sau đó, chưa hết 1200 ms thực

    expect(candidates()).toHaveLength(1);
  });

  it('core từ chối probe thì mở cổng ngay cho câu Hey Liva kế tiếp', async () => {
    await import('../../src/workers/LivaWakeWorker');
    const onmessage = (globalThis.self as any).onmessage;
    await onmessage({ data: { type: 'init' } });

    const utterance = [
      ...Array.from({ length: 20 }, () => speechFrame()),
      ...Array.from({ length: 8 }, silentFrame),
    ];
    await feed(onmessage, utterance);
    await feed(onmessage, utterance);
    expect(candidates()).toHaveLength(1);

    await onmessage({
      data: { type: 'probeResult', data: { accepted: false } },
    });
    await feed(onmessage, utterance);

    expect(candidates()).toHaveLength(2);
  });

  it('xử lý pause, resume, reset, setThreshold, terminate', async () => {
    await import('../../src/workers/LivaWakeWorker');
    const onmessage = (globalThis.self as any).onmessage;

    await onmessage({ data: { type: 'pause' } });
    expect(postMessageMock).toHaveBeenCalledWith({ type: 'paused' });

    await onmessage({ data: { type: 'resume' } });
    expect(postMessageMock).toHaveBeenCalledWith({ type: 'resumed' });

    await onmessage({ data: { type: 'reset' } });
    expect(postMessageMock).toHaveBeenCalledWith({ type: 'reset' });

    await onmessage({ data: { type: 'setThreshold', data: { threshold: 0.03 } } });
    expect(postMessageMock).toHaveBeenCalledWith({ type: 'thresholdChanged', threshold: 0.03 });

    await onmessage({ data: { type: 'terminate' } });
    expect(postMessageMock).toHaveBeenCalledWith({ type: 'terminated' });
    expect(closeMock).toHaveBeenCalled();
  });

  it('pause thì không cắt câu nữa', async () => {
    await import('../../src/workers/LivaWakeWorker');
    const onmessage = (globalThis.self as any).onmessage;
    await onmessage({ data: { type: 'init' } });
    await onmessage({ data: { type: 'pause' } });

    await feed(onmessage, [
      ...Array.from({ length: 20 }, () => speechFrame()),
      ...Array.from({ length: 8 }, silentFrame),
    ]);

    expect(candidates()).toHaveLength(0);
  });
});
