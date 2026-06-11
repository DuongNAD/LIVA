import { parentPort } from "node:worker_threads";

let tts: any = null;

parentPort?.on("message", async (msg: { type: string; id?: string; text?: string; voice?: string; modelId?: string; dtype?: string }) => {
    switch (msg.type) {
        case "init":
            try {
                const modelId = msg.modelId || "onnx-community/Kokoro-82M-v1.0-ONNX";
                const dtype = (msg.dtype || "q8") as "q8" | "fp32" | "q4" | "fp16" | "q4f16";

                // Dynamically import kokoro-js ONLY when initializing
                const { KokoroTTS } = await import("kokoro-js");

                // Use device: "cpu" on macOS to prevent native SIGSEGV (exit code 139)
                tts = await KokoroTTS.from_pretrained(modelId, {
                    dtype,
                    device: "cpu"
                });

                tts.list_voices();

                parentPort?.postMessage({ type: "ready" });
            } catch (err: unknown) {
                const errMsg = err instanceof Error ? err.message : String(err);
                parentPort?.postMessage({ type: "error", message: `Worker init failed: ${errMsg}` });
            }
            break;

        case "generate":
            try {
                if (!tts) {
                    throw new Error("TTS model not initialized in worker");
                }
                const voice = msg.voice || "af_heart";
                const audio = await tts.generate(msg.text!, {
                    voice
                });
                const wavBuffer = audio.toWav();
                parentPort?.postMessage({ type: "generate_result", id: msg.id!, wavBuffer });
            } catch (err: unknown) {
                const errMsg = err instanceof Error ? err.message : String(err);
                parentPort?.postMessage({ type: "error", id: msg.id!, message: `Generate failed: ${errMsg}` });
            }
            break;

        case "dispose":
            tts = null;
            process.exit(0);
            break;
    }
});
