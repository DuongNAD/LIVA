import { vi } from 'vitest'
import url from 'url'

// Stub WebSocket globally to prevent real network connections during jsdom tests
class MockWebSocket {
  static OPEN = 1;
  static CONNECTING = 0;
  static CLOSING = 2;
  static CLOSED = 3;

  OPEN = 1;
  CONNECTING = 0;
  CLOSING = 2;
  CLOSED = 3;

  readyState = MockWebSocket.CLOSED;
  binaryType = '';
  onopen: ((ev: any) => void) | null = null;
  onmessage: ((ev: any) => void) | null = null;
  onclose: ((ev: any) => void) | null = null;
  onerror: ((ev: any) => void) | null = null;
  send = vi.fn();
  close = vi.fn();
}

vi.stubGlobal('WebSocket', MockWebSocket);


const originalFileURLToPath = url.fileURLToPath;
url.fileURLToPath = function (fileUrl: any) {
  try {
    return originalFileURLToPath(fileUrl);
  } catch (err) {
    return 'C:\\dummy_path';
  }
};

const originalGetContext = HTMLCanvasElement.prototype.getContext;

HTMLCanvasElement.prototype.getContext = function(type: string, contextAttributes?: any) {
  if (type !== '2d') {
    return {
      getExtension: vi.fn(),
      getParameter: vi.fn(),
      canvas: this
    } as any;
  }
  return {
    fillRect: vi.fn(),
    clearRect: vi.fn(),
    getImageData: vi.fn(() => ({ data: new Uint8ClampedArray(4) })),
    putImageData: vi.fn(),
    createImageData: vi.fn(() => []),
    setTransform: vi.fn(),
    drawImage: vi.fn(),
    save: vi.fn(),
    fillText: vi.fn(),
    restore: vi.fn(),
    beginPath: vi.fn(),
    moveTo: vi.fn(),
    lineTo: vi.fn(),
    closePath: vi.fn(),
    stroke: vi.fn(),
    translate: vi.fn(),
    scale: vi.fn(),
    rotate: vi.fn(),
    arc: vi.fn(),
    fill: vi.fn(),
    measureText: vi.fn(() => ({ width: 0 })),
    transform: vi.fn(),
    rect: vi.fn(),
    clip: vi.fn(),
    canvas: this
  } as any;
};

// WebGL mocks
HTMLCanvasElement.prototype.toDataURL = vi.fn();
