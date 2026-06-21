const ort = require('onnxruntime-node');
const path = require('path');

async function main() {
    const modelDir = 'e:\\Project\\LIVA\\liva-gateway\\models\\nemotron-asr';
    const encoderPath = path.join(modelDir, 'encoder.onnx');
    const decoderPath = path.join(modelDir, 'decoder.onnx');
    const joinerPath = path.join(modelDir, 'joint.onnx');

    console.log("Loading sessions...");
    const encoder = await ort.InferenceSession.create(encoderPath);
    const decoder = await ort.InferenceSession.create(decoderPath);
    const joiner = await ort.InferenceSession.create(joinerPath);

    console.log("Running Decoder...");
    const targets = new ort.Tensor("int64", BigInt64Array.from([13087n]), [1, 1]); // SOS/blank token
    const h_in = new ort.Tensor("float32", new Float32Array(2 * 1 * 640).fill(0), [2, 1, 640]);
    const c_in = new ort.Tensor("float32", new Float32Array(2 * 1 * 640).fill(0), [2, 1, 640]);

    const decResults = await decoder.run({ targets, h_in, c_in });
    const decoderOut = decResults["decoder_output"];
    console.log("Decoder output shape:", decoderOut.dims); // [1, 640, 1]

    console.log("Reshaping decoder output...");
    const reshapedDecoderOut = new ort.Tensor(
        "float32",
        decoderOut.data,
        [1, 1, 640]
    );
    console.log("Reshaped shape:", reshapedDecoderOut.dims);

    console.log("Running Joiner...");
    const dummyEnc = new ort.Tensor("float32", new Float32Array(1024).fill(0), [1, 1, 1024]);
    
    const jointResults = await joiner.run({
        encoder_output: dummyEnc,
        decoder_output: reshapedDecoderOut
    });
    
    const logits = jointResults["joint_output"];
    console.log("Joiner output shape:", logits.dims); // [1, 1, 1, 13088]
    console.log("Successfully ran Joiner without crash!");
}

main().catch(console.error);
