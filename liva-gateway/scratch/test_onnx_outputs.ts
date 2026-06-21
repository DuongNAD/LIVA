import * as dotenv from "dotenv";
dotenv.config();

import * as ort from "onnxruntime-node";
import * as path from "node:path";
import * as fs from "node:fs";

// Load genai_config
const modelDir = "e:\\Project\\LIVA\\liva-gateway\\models\\nemotron-asr";
const configPath = path.join(modelDir, "genai_config.json");
const config = JSON.parse(fs.readFileSync(configPath, "utf-8")).model;

async function main() {
    console.log("Loading sessions...");
    const encoder = await ort.InferenceSession.create(path.join(modelDir, "encoder.onnx"));
    const decoder = await ort.InferenceSession.create(path.join(modelDir, "decoder.onnx"));
    const joint = await ort.InferenceSession.create(path.join(modelDir, "joint.onnx"));
    
    console.log("ONNX Sessions loaded successfully!");

    // Let's create dummy mel features of shape [1, 65, 128]
    // Fill with simulated audio features (e.g. constant values or noise)
    const numFrames = 65;
    const numMels = 128;
    const features = new Float32Array(numFrames * numMels).fill(-2.0); // typical log-mel quiet value

    const featureTensor = new ort.Tensor("float32", features, [1, numFrames, numMels]);
    const lengthTensor = new ort.Tensor("int64", BigInt64Array.from([BigInt(numFrames)]), [1]);
    
    // cache_last_channel: [1, 24, 56, 1024]
    const cacheLastChannel = new Float32Array(1 * 24 * 56 * 1024).fill(0);
    const cacheChannelTensor = new ort.Tensor("float32", cacheLastChannel, [1, 24, 56, 1024]);
    
    // cache_last_time: [1, 24, 1024, 8]
    const cacheLastTime = new Float32Array(1 * 24 * 1024 * 8).fill(0);
    const cacheTimeTensor = new ort.Tensor("float32", cacheLastTime, [1, 24, 1024, 8]);
    
    const cacheChannelLenTensor = new ort.Tensor("int64", new BigInt64Array([0n]), [1]);
    const langIdTensor = new ort.Tensor("int64", BigInt64Array.from([33n]), [1]);

    console.log("Running encoder...");
    const encResults = await encoder.run({
        audio_signal: featureTensor,
        length: lengthTensor,
        cache_last_channel: cacheChannelTensor,
        cache_last_time: cacheTimeTensor,
        cache_last_channel_len: cacheChannelLenTensor,
        lang_id: langIdTensor
    });

    const encoderOut = encResults["outputs"];
    const outLenTensor = encResults["encoded_lengths"];
    
    console.log("--- ENCODER OUTPUTS ---");
    console.log("outputs dims:", encoderOut.dims);
    console.log("encoded_lengths:", outLenTensor ? (outLenTensor.data as BigInt64Array)[0].toString() : "N/A");
    
    const encData = encoderOut.data as Float32Array;
    console.log("outputs sample values (first 10):", Array.from(encData.slice(0, 10)));
    
    // Let's run decoder
    console.log("\nRunning decoder...");
    // targets: [1, 1]
    const targets = new ort.Tensor("int64", BigInt64Array.from([13087n]), [1, 1]); // blank token
    const hIn = new ort.Tensor("float32", new Float32Array(2 * 1 * 640).fill(0), [2, 1, 640]);
    const cIn = new ort.Tensor("float32", new Float32Array(2 * 1 * 640).fill(0), [2, 1, 640]);
    
    const decResults = await decoder.run({
        targets,
        h_in: hIn,
        c_in: cIn
    });
    
    const decoderOut = decResults["decoder_output"];
    console.log("decoder_output dims:", decoderOut.dims);
    console.log("decoder sample values (first 10):", Array.from((decoderOut.data as Float32Array).slice(0, 10)));

    // Let's run joint
    console.log("\nRunning joint...");
    // Extract single frame of encoder output: [1, 1, 1024]
    const singleEncFrame = new Float32Array(1024);
    singleEncFrame.set(encData.subarray(0, 1024));
    
    const encFrameTensor = new ort.Tensor("float32", singleEncFrame, [1, 1, 1024]);
    
    // decoder_output is [1, 1, 640], let's take a single frame
    const singleDecFrame = new Float32Array(640);
    singleDecFrame.set((decoderOut.data as Float32Array).subarray(0, 640));
    const decFrameTensor = new ort.Tensor("float32", singleDecFrame, [1, 1, 640]);

    const jointResults = await joint.run({
        encoder_output: encFrameTensor,
        decoder_output: decFrameTensor
    });
    
    const logits = jointResults["joint_output"];
    console.log("joint_output dims:", logits.dims);
    
    const logitsData = logits.data as Float32Array;
    let maxIdx = 0;
    let maxVal = logitsData[0];
    for (let i = 1; i < logitsData.length; i++) {
        if (logitsData[i] > maxVal) {
            maxVal = logitsData[i];
            maxIdx = i;
        }
    }
    console.log(`Max logit index (predicted token): ${maxIdx}, val: ${maxVal}`);
}

main().catch(console.error);
