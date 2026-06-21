import { parentPort } from "node:worker_threads";

let tts: any = null;
let isReady = false;

async function initModel(modelId: string, dtype: string) {
    try {
        const { KokoroTTS } = await import("kokoro-js");
        tts = await KokoroTTS.from_pretrained(modelId, {
            dtype: dtype as any,
            device: "cpu",
        });
        isReady = true;
        tts.list_voices();
        parentPort?.postMessage({ type: "ready" });
    } catch (e: unknown) {
        const errMsg = e instanceof Error ? e.message : String(e);
        parentPort?.postMessage({ type: "error", message: errMsg });
    }
}

async function generate(text: string, voice: string) {
    if (!tts || !isReady) {
        parentPort?.postMessage({ type: "error", message: "KokoroTTS is not initialized" });
        return;
    }
    try {
        const audio = await tts.generate(text, { voice });
        const wavBuffer = audio.toWav();
        const base64 = Buffer.from(wavBuffer).toString("base64");
        parentPort?.postMessage({ type: "audio_result", base64 });
    } catch (e: unknown) {
        const errMsg = e instanceof Error ? e.message : String(e);
        parentPort?.postMessage({ type: "error", message: errMsg });
    }
}

parentPort?.on("message", async (msg: { type: string; modelId?: string; dtype?: string; text?: string; voice?: string }) => {
    switch (msg.type) {
        case "init":
            await initModel(msg.modelId!, msg.dtype!);
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
