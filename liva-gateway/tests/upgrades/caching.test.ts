import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import path from "node:path";

// 1. Mock @grpc/grpc-js and @grpc/proto-loader
const mockChat = vi.fn();
const mockStreamChat = vi.fn();
const mockHealthCheck = vi.fn();
const mockSwapModel = vi.fn();
const mockClose = vi.fn();
let capturedOptions: any = null;

vi.mock("@grpc/grpc-js", () => {
  class MockLivaInferenceService {
    constructor(host: string, creds: any, options: any) {
      capturedOptions = options;
    }
    Chat = mockChat;
    StreamChat = mockStreamChat;
    HealthCheck = mockHealthCheck;
    SwapModel = mockSwapModel;
    close = mockClose;
  }
  return {
    loadPackageDefinition: () => ({
      liva: {
        LivaInferenceService: MockLivaInferenceService,
      },
    }),
    credentials: {
      createInsecure: vi.fn(),
    },
  };
});

vi.mock("@grpc/proto-loader", () => ({
  loadSync: vi.fn().mockReturnValue({}),
}));

vi.mock("child_process", () => {
  const mockCp = {
    spawn: vi.fn().mockReturnValue({
      pid: 9999,
      stdout: { on: vi.fn() },
      stderr: { on: vi.fn() },
      on: vi.fn(),
      kill: vi.fn(),
    }),
    execSync: vi.fn().mockImplementation(() => {
      throw new Error("command not found");
    }),
  };
  return {
    ...mockCp,
    default: mockCp,
  };
});

vi.mock("fs", async (importOriginal) => {
  const actual = await importOriginal<typeof import("fs")>();
  const mockExistsSync = vi.fn().mockImplementation((p: string) => {
    if (p.includes("llama-server") || p.includes("gemma") || p.includes("draft") || p.includes("python")) {
      return true;
    }
    return false;
  });
  return {
    ...actual,
    existsSync: mockExistsSync,
    default: {
      ...actual.default,
      existsSync: mockExistsSync,
    },
  };
});

describe("Persistent Prompt Caching Tests", () => {
  const originalEnv = { ...process.env };

  beforeEach(() => {
    vi.clearAllMocks();
    vi.resetModules();
    capturedOptions = null;
  });

  afterEach(() => {
    process.env = { ...originalEnv };
  });

  it("should configure --cache-reuse 256 in ModelOrchestrator llama-server process", async () => {
    process.env.LIVA_USE_NATIVE = "false";
    process.env.AI_MODELS_DIR = "E:\\AI_Models";
    process.env.EXPERT_MODEL_NAME = "gemma-expert.gguf";

    const { ModelOrchestrator } = await import("../../src/core/ModelOrchestrator");
    const cpMock = await import("child_process");
    const orchestrator = new ModelOrchestrator();

    await orchestrator.startSingleExpert();

    expect(cpMock.spawn).toHaveBeenCalled();
    const spawnCall = vi.mocked(cpMock.spawn).mock.calls[0];
    const args = spawnCall[1];
    expect(args).toContain("--cache-reuse");
    expect(args).toContain("256");

    await orchestrator.dispose();
  });

  it("should initialize NativeIPCClient with correct gRPC keepalive and proxy options", async () => {
    const { NativeIPCClient } = await import("../../src/utils/NativeIPCClient");
    new NativeIPCClient();

    expect(capturedOptions).toBeDefined();
    expect(capturedOptions["grpc.keepalive_time_ms"]).toBe(30000);
    expect(capturedOptions["grpc.keepalive_timeout_ms"]).toBe(10000);
    expect(capturedOptions["grpc.keepalive_permit_without_calls"]).toBe(1);
    expect(capturedOptions["grpc.max_receive_message_length"]).toBe(50 * 1024 * 1024);
    expect(capturedOptions["grpc.enable_http_proxy"]).toBe(0);
  });

  it("should execute swapModel on the gRPC client successfully", async () => {
    const { NativeIPCClient } = await import("../../src/utils/NativeIPCClient");
    const client = new NativeIPCClient();

    const mockResponse = {
      success: true,
      error_message: "",
      loaded_model: "gemma-expert.gguf",
      swap_duration_ms: 1200,
    };

    mockSwapModel.mockImplementation((req: any, cb: Function) => cb(null, mockResponse));

    const result = await client.swapModel("E:\\AI_Models\\gemma-expert.gguf", 2048, 16);

    expect(mockSwapModel).toHaveBeenCalled();
    const sentReq = mockSwapModel.mock.calls[0][0];
    expect(sentReq.model_path).toBe("E:\\AI_Models\\gemma-expert.gguf");
    expect(sentReq.n_ctx).toBe(2048);
    expect(sentReq.n_gpu_layers).toBe(16);

    expect(result.success).toBe(true);
    expect(result.loadedModel).toBe("gemma-expert.gguf");
    expect(result.swapDurationMs).toBe(1200);
  });

  it("should execute StreamChat on the gRPC client during streaming calls", async () => {
    const { NativeIPCClient } = await import("../../src/utils/NativeIPCClient");
    const client = new NativeIPCClient();

    const { EventEmitter } = await import("node:events");
    const mockStream = new EventEmitter();
    mockStreamChat.mockReturnValue(mockStream);

    const streamPromise = client.chat.completions.create({
      messages: [{ role: "user", content: "test caching" }],
      stream: true,
    });

    setTimeout(() => {
      mockStream.emit("data", {
        id: "chunk_1",
        choices: [{ index: 0, delta: { content: "Cached response" }, finish_reason: null }],
      });
      mockStream.emit("end");
    }, 10);

    const stream = await streamPromise;
    expect(mockStreamChat).toHaveBeenCalled();

    const sentReq = mockStreamChat.mock.calls[0][0];
    expect(sentReq.messages).toEqual([{ role: "user", content: "test caching" }]);
    expect(sentReq.stream).toBe(true);

    const chunks: any[] = [];
    for await (const chunk of stream) {
      chunks.push(chunk);
    }

    expect(chunks).toHaveLength(1);
    expect(chunks[0].choices[0].delta.content).toBe("Cached response");
  });

  it("should automatically retry unary calls on 14 UNAVAILABLE errors up to 3 times", async () => {
    const { NativeIPCClient } = await import("../../src/utils/NativeIPCClient");
    const client = new NativeIPCClient();

    let callCount = 0;
    mockChat.mockImplementation((req: any, cb: Function) => {
      callCount++;
      if (callCount === 1) {
        const err = new Error("14 UNAVAILABLE: Service unavailable") as any;
        cb(err, null);
      } else {
        cb(null, { id: "res_ok", choices: [{ message: { role: "assistant", content: "Success" } }] });
      }
    });

    const result = await client.chat.completions.create({
      messages: [{ role: "user", content: "test retry" }],
      stream: false,
    });

    expect(callCount).toBe(2);
    expect((result as any).choices[0].message.content).toBe("Success");
  });

  it("should rollback to router when swapToExpert fails", async () => {
    const { ModelOrchestrator } = await import("../../src/core/ModelOrchestrator");
    const orchestrator = new ModelOrchestrator();
    const swapToRouterSpy = vi.spyOn(orchestrator, "swapToRouter");

    mockSwapModel.mockImplementation((req: any, cb: Function) => {
      cb(null, {
        success: false,
        error_message: "VRAM allocation failed",
        loaded_model: "",
        swap_duration_ms: 0,
      });
    });

    process.env.LIVA_USE_NATIVE = "true";
    process.env.AI_MODELS_DIR = "E:\\AI_Models";
    process.env.EXPERT_MODEL_NAME = "gemma-expert.gguf";

    const result = await orchestrator.swapToExpert();
    expect(result).toBe(false);
    expect(swapToRouterSpy).toHaveBeenCalled();

    await orchestrator.dispose();
  });
});
