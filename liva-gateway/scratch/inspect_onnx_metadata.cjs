const ort = require('onnxruntime-node');
const path = require('path');

async function main() {
    const modelDir = 'e:\\Project\\LIVA\\liva-gateway\\models\\nemotron-asr';
    const encoderPath = path.join(modelDir, 'encoder.onnx');
    const decoderPath = path.join(modelDir, 'decoder.onnx');
    const joinerPath = path.join(modelDir, 'joint.onnx');

    console.log("=== INSPECTING ONNX MODELS ===");

    try {
        console.log("\nLoading Encoder...");
        const encoder = await ort.InferenceSession.create(encoderPath);
        console.log("Encoder Inputs:");
        for (const name of encoder.inputNames) {
            console.log(`  - ${name}`);
        }
        console.log("Encoder Outputs:");
        for (const name of encoder.outputNames) {
            console.log(`  - ${name}`);
        }
    } catch (e) {
        console.error("Failed to load Encoder:", e.message);
    }

    try {
        console.log("\nLoading Decoder...");
        const decoder = await ort.InferenceSession.create(decoderPath);
        console.log("Decoder Inputs:");
        for (const name of decoder.inputNames) {
            console.log(`  - ${name}`);
        }
        console.log("Decoder Outputs:");
        for (const name of decoder.outputNames) {
            console.log(`  - ${name}`);
        }
    } catch (e) {
        console.error("Failed to load Decoder:", e.message);
    }

    try {
        console.log("\nLoading Joiner/Joint...");
        const joiner = await ort.InferenceSession.create(joinerPath);
        console.log("Joiner Inputs:");
        for (const name of joiner.inputNames) {
            console.log(`  - ${name}`);
        }
        console.log("Joiner Outputs:");
        for (const name of joiner.outputNames) {
            console.log(`  - ${name}`);
        }
    } catch (e) {
        console.error("Failed to load Joiner/Joint:", e.message);
    }
}

main().catch(console.error);
