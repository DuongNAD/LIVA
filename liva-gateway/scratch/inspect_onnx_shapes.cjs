const ort = require('onnxruntime-node');
const path = require('path');
const inspect = require('util').inspect;

async function main() {
    const modelDir = 'e:\\Project\\LIVA\\liva-gateway\\models\\nemotron-asr';
    const encoderPath = path.join(modelDir, 'encoder.onnx');
    const decoderPath = path.join(modelDir, 'decoder.onnx');
    const joinerPath = path.join(modelDir, 'joint.onnx');

    const encoder = await ort.InferenceSession.create(encoderPath);
    const decoder = await ort.InferenceSession.create(decoderPath);
    const joiner = await ort.InferenceSession.create(joinerPath);

    console.log("=== ENCODER DETAILS ===");
    console.log(inspect(encoder, { depth: null }));

    console.log("=== DECODER DETAILS ===");
    console.log(inspect(decoder, { depth: null }));

    console.log("=== JOINER DETAILS ===");
    console.log(inspect(joiner, { depth: null }));
}

main().catch(console.error);
