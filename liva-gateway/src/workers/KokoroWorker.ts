import { parentPort } from "node:worker_threads";
import type { KokoroTTS, GenerateOptions } from "kokoro-js";

let tts: KokoroTTS | null = null;
let isReady = false;

let currentModelId = "onnx-community/Kokoro-82M-v1.0-ONNX";
let currentDtype: "fp32" | "fp16" | "q8" | "q4" | "q4f16" = "q8";

async function initModel(modelId: string, dtype: "fp32" | "fp16" | "q8" | "q4" | "q4f16", device: string) {
    currentModelId = modelId;
    currentDtype = dtype;
    const { KokoroTTS } = await import("kokoro-js");

    const devicesToTry: string[] = [];
    const targetDevice: string = device === "directml" ? "dml" : device;
    devicesToTry.push(targetDevice);

    if (targetDevice !== "dml" && targetDevice !== "cpu") {
        devicesToTry.push("dml");
    }
    if (targetDevice !== "cpu") {
        devicesToTry.push("cpu");
    }

    let lastError: unknown = null;
    for (const d of devicesToTry) {
        try {
            parentPort?.postMessage({ type: "log", level: "info", message: `Trying to initialize KokoroTTS on device: ${d}...` });
            tts = await KokoroTTS.from_pretrained(modelId, {
                dtype,
                device: d as unknown as "wasm" | "webgpu" | "cpu" | null,
            });
            isReady = true;
            tts.list_voices();
            parentPort?.postMessage({ type: "log", level: "info", message: `✅ KokoroTTS initialized successfully on device: ${d}` });
            parentPort?.postMessage({ type: "ready" });
            return;
        } catch (err: unknown) {
            lastError = err;
            const errStr = err instanceof Error ? err.message : String(err);
            parentPort?.postMessage({ type: "log", level: "warn", message: `⚠️ Failed to initialize KokoroTTS on device ${d}: ${errStr}` });
        }
    }

    const errMsg = lastError instanceof Error ? lastError.message : String(lastError);
    parentPort?.postMessage({ type: "error", message: `All devices failed to initialize. Last error: ${errMsg}` });
}

async function generate(text: string, voice: string) {
    if (!tts || !isReady) {
        parentPort?.postMessage({ type: "error", message: "KokoroTTS is not initialized" });
        return;
    }
    try {
        const audio = await tts.generate(text, { voice: voice as GenerateOptions["voice"] });
        const wavBuffer = audio.toWav();
        const base64 = Buffer.from(wavBuffer).toString("base64");
        parentPort?.postMessage({ type: "audio_result", base64 });
    } catch (e: unknown) {
        const errMsg = e instanceof Error ? e.message : String(e);
        if (errMsg.includes("dml") || errMsg.includes("DmlExecutionProvider") || errMsg.includes("ConvTranspose") || errMsg.includes("parameter is incorrect")) {
            parentPort?.postMessage({ type: "log", level: "warn", message: "⚠️ DirectML execution failed. Falling back to CPU..." });
            try {
                const { KokoroTTS } = await import("kokoro-js");
                tts = await KokoroTTS.from_pretrained(currentModelId, {
                    dtype: currentDtype,
                    device: "cpu",
                });
                const audio = await tts.generate(text, { voice: voice as GenerateOptions["voice"] });
                const wavBuffer = audio.toWav();
                const base64 = Buffer.from(wavBuffer).toString("base64");
                parentPort?.postMessage({ type: "audio_result", base64 });
                return;
            } catch (fallbackErr: unknown) {
                const fallbackErrMsg = fallbackErr instanceof Error ? fallbackErr.message : String(fallbackErr);
                parentPort?.postMessage({ type: "log", level: "error", message: `Fallback to CPU failed: ${fallbackErrMsg}` });
                parentPort?.postMessage({ type: "error", message: `Fallback to CPU failed: ${fallbackErrMsg}` });
                return;
            }
        }
        parentPort?.postMessage({ type: "error", message: errMsg });
    }
}

parentPort?.on("message", async (msg: { type: string; modelId?: string; dtype?: string; device?: string; text?: string; voice?: string }) => {
    switch (msg.type) {
        case "init":
            await initModel(msg.modelId!, msg.dtype! as "fp32" | "fp16" | "q8" | "q4" | "q4f16", msg.device || "cpu");
            break;
        case "generate":
            await generate(msg.text!, msg.voice!);
            break;
        case "dispose":
            tts = null;
            isReady = false;
            process.exit(0);
            break;
    }
});
