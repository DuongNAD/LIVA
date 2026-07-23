import { logger } from './logger';

export enum OpCode {
  OP_AUTH_HANDSHAKE = 0x00,
  OP_MIC_IN = 0x01,
  OP_SPEAKER_OUT = 0x02,
  OP_FLUSH = 0x03,
  OP_ACK_PLAYING = 0x04,
}

export interface VoiceFrame {
  opcode: OpCode;
  seqId: number;
  payloadSize: number;
  payload: Uint8Array;
}

export interface IpcRequest {
  id: string;
  command: string;
  payload: unknown;
}

export interface IpcResponse {
  id: string;
  status: 'ok' | 'error';
  data?: unknown;
  error?: string;
}

type MessageCallback = (data: IpcResponse) => void;
type VoiceFrameCallback = (frame: VoiceFrame) => void;

export class WebSocketClient {
  private ws: WebSocket | null = null;
  private url: string;
  private pendingRequests = new Map<string, { resolve: (val: unknown) => void; reject: (err: Error) => void }>();
  private messageCallbacks = new Set<MessageCallback>();
  private voiceFrameCallbacks = new Set<VoiceFrameCallback>();
  private onConnectCallbacks = new Set<() => void>();
  private onDisconnectCallbacks = new Set<() => void>();
  private onErrorCallbacks = new Set<(err: Event) => void>();
  private minimumSpeakerEpoch = 0;

  constructor(url: string) {
    this.url = url;
  }

  private clearSocketListeners(): void {
    if (this.ws) {
      this.ws.onopen = null;
      this.ws.onmessage = null;
      this.ws.onclose = null;
      this.ws.onerror = null;
    }
  }

  public connect(): Promise<void> {
    return new Promise((resolve, reject) => {
      try {
        this.minimumSpeakerEpoch = 0;
        this.ws = new WebSocket(this.url);
        this.ws.binaryType = 'arraybuffer';

        this.ws.onopen = () => {
          logger.log(`WebSocket connected to ${this.url}`);
          for (const cb of this.onConnectCallbacks) {
            cb();
          }
          resolve();
        };

        this.ws.onmessage = (event) => {
          this.handleMessage(event);
        };

        this.ws.onclose = () => {
          logger.log('WebSocket connection closed');
          this.clearSocketListeners();
          for (const cb of this.onDisconnectCallbacks) {
            cb();
          }
          this.cleanupPendingRequests('Connection closed');
          this.ws = null;
        };

        this.ws.onerror = (err) => {
          logger.error('WebSocket error:', err);
          for (const cb of this.onErrorCallbacks) {
            cb(err);
          }
          reject(err);
        };
      } catch (err) {
        reject(err as Error);
      }
    });
  }

  public disconnect(): void {
    if (this.ws) {
      const socket = this.ws;
      this.clearSocketListeners();
      socket.close();
      this.ws = null;
    }
  }

  public isConnected(): boolean {
    return this.ws !== null && this.ws.readyState === WebSocket.OPEN;
  }

  // --- Event registration ---
  public onConnect(cb: () => void) { this.onConnectCallbacks.add(cb); }
  public onDisconnect(cb: () => void) { this.onDisconnectCallbacks.add(cb); }
  public onError(cb: (err: Event) => void) { this.onErrorCallbacks.add(cb); }
  public onMessage(cb: MessageCallback) { this.messageCallbacks.add(cb); }
  public onVoiceFrame(cb: VoiceFrameCallback) { this.voiceFrameCallbacks.add(cb); }

  // --- Handlers ---
  private handleMessage(event: MessageEvent) {
    if (typeof event.data === 'string') {
      try {
        const response: IpcResponse = JSON.parse(event.data);
        if (response.id && this.pendingRequests.has(response.id)) {
          const { resolve, reject } = this.pendingRequests.get(response.id)!;
          this.pendingRequests.delete(response.id);
          if (response.status === 'ok') {
            resolve(response.data);
          } else {
            reject(new Error(response.error || 'Unknown server error'));
          }
        }
        for (const cb of this.messageCallbacks) {
          cb(response);
        }
      } catch (err) {
        logger.error('Failed to parse text message:', err);
      }
    } else if (event.data instanceof ArrayBuffer) {
      try {
        const frame = this.deserializeVoiceFrame(event.data);
        if (frame.opcode === OpCode.OP_FLUSH) {
          this.minimumSpeakerEpoch = Math.max(this.minimumSpeakerEpoch, frame.seqId);
        } else if (frame.opcode === OpCode.OP_SPEAKER_OUT) {
          if (frame.payload.byteLength < 4) return;
          const turnEpoch = new DataView(
            frame.payload.buffer,
            frame.payload.byteOffset,
            frame.payload.byteLength,
          ).getUint32(0, true);
          if (turnEpoch < this.minimumSpeakerEpoch) return;
        }
        for (const cb of this.voiceFrameCallbacks) {
          cb(frame);
        }
      } catch (err) {
        logger.error('Failed to parse binary message:', err);
      }
    }
  }

  private cleanupPendingRequests(reason: string) {
    for (const { reject } of this.pendingRequests.values()) {
      reject(new Error(reason));
    }
    this.pendingRequests.clear();
  }

  // --- Sending JSON commands ---
  public sendJsonCommand(command: string, payload: unknown = {}): Promise<unknown> {
    return new Promise((resolve, reject) => {
      if (!this.isConnected()) {
        return reject(new Error('WebSocket is not connected'));
      }
      const id = crypto.randomUUID ? crypto.randomUUID() : Math.random().toString(36).substring(2);
      const request: IpcRequest = { id, command, payload };
      this.pendingRequests.set(id, { resolve, reject });
      try {
        this.ws!.send(JSON.stringify(request));
      } catch (err) {
        this.pendingRequests.delete(id);
        reject(err instanceof Error ? err : new Error(String(err)));
      }
    });
  }

  // --- Sending Binary voice frames ---
  public sendVoiceFrame(opcode: OpCode, seqId: number, payload: Uint8Array): void {
    if (!this.isConnected()) {
      throw new Error('WebSocket is not connected');
    }
    const data = this.serializeVoiceFrame(opcode, seqId, payload);
    this.ws!.send(data as unknown as BufferSource);
  }

  // --- OP_AUTH_HANDSHAKE helper ---
  public sendAuthHandshake(seqId: number, token: string = 'auth_token'): Promise<VoiceFrame> {
    return new Promise((resolve, reject) => {
      const payload = new TextEncoder().encode(token);
      
      const onFrame = (frame: VoiceFrame) => {
        if (frame.opcode === OpCode.OP_AUTH_HANDSHAKE && frame.seqId === seqId) {
          cleanup();
          resolve(frame);
        }
      };

      const onDisconnect = () => {
        cleanup();
        reject(new Error('Connection closed during handshake'));
      };

      const onError = () => {
        cleanup();
        reject(new Error('Connection error during handshake'));
      };

      const cleanup = () => {
        this.voiceFrameCallbacks.delete(onFrame);
        this.onDisconnectCallbacks.delete(onDisconnect);
        this.onErrorCallbacks.delete(onError);
      };

      this.onVoiceFrame(onFrame);
      this.onDisconnect(onDisconnect);
      this.onError(onError);

      try {
        this.sendVoiceFrame(OpCode.OP_AUTH_HANDSHAKE, seqId, payload);
      } catch (err) {
        cleanup();
        reject(err instanceof Error ? err : new Error(String(err)));
      }
    });
  }

  // --- Serialization / Deserialization helpers ---
  private serializeVoiceFrame(opcode: OpCode, seqId: number, payload: Uint8Array): Uint8Array {
    const payloadSize = payload.length;
    const buffer = new ArrayBuffer(9 + payloadSize);
    const view = new DataView(buffer);
    
    view.setUint8(0, opcode);
    view.setUint32(1, seqId, true); // little-endian
    view.setUint32(5, payloadSize, true); // little-endian
    
    const uint8View = new Uint8Array(buffer);
    uint8View.set(payload, 9);
    return uint8View;
  }

  private deserializeVoiceFrame(buffer: ArrayBuffer): VoiceFrame {
    if (buffer.byteLength < 9) {
      throw new Error(`Buffer is too small: ${buffer.byteLength} bytes`);
    }
    const view = new DataView(buffer);
    const opcode = view.getUint8(0) as OpCode;
    const seqId = view.getUint32(1, true); // little-endian
    const payloadSize = view.getUint32(5, true); // little-endian
    
    if (buffer.byteLength < 9 + payloadSize) {
      throw new Error(`Buffer size mismatch. Expected ${9 + payloadSize} bytes, got ${buffer.byteLength}`);
    }
    
    const payload = new Uint8Array(buffer, 9, payloadSize);
    return { opcode, seqId, payloadSize, payload };
  }
}
