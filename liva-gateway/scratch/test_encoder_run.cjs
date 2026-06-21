const ort = require('onnxruntime-node');
const path = require('path');

async function main() {
    const modelDir = 'e:\\Project\\LIVA\\liva-gateway\\models\\nemotron-asr';
    const encoderPath = path.join(modelDir, 'encoder.onnx');
    const encoder = await ort.InferenceSession.create(encoderPath);

    console.log("Running Encoder...");
    const numFrames = 65;
    const numMels = 128;
    const features = new Float32Array(numFrames * numMels).fill(0);
    const featureTensor = new ort.Tensor("float32", features, [1, numFrames, numMels]);
    const lengthTensor = new ort.Tensor("int64", BigInt64Array.from([BigInt(numFrames)]), [1]);

    const channelElements = 24 * 1 * 1024 * 56;
    const cacheLastChannel = new Float32Array(channelElements).fill(0);
    const cacheChannelTensor = new ort.Tensor("float32", cacheLastChannel, [1, 24, 56, 1024]);

    const timeElements = 24 * 1 * 1024 * 8;
    const cacheLastTime = new Float32Array(timeElements).fill(0);
    const cacheTimeTensor = new ort.Tensor("float32", cacheLastTime, [1, 24, 1024, 8]);

    const cacheChannelLenTensor = new ort.Tensor("int64", new BigInt64Array([0n]), [1]);
    const langIdTensor = new ort.Tensor("int64", BigInt64Array.from([33n]), [1]);

    const results = await encoder.run({
        audio_signal: featureTensor,
        length: lengthTensor,
        cache_last_channel: cacheChannelTensor,
        cache_last_time: cacheTimeTensor,
        cache_last_channel_len: cacheChannelLenTensor,
        lang_id: langIdTensor
    });

    console.log("Encoder output keys:", Object.keys(results));
    console.log("outputs shape:", results["outputs"].dims);
    console.log("encoded_lengths value:", results["encoded_lengths"].data);
    console.log("cache_last_channel_next shape:", results["cache_last_channel_next"].dims);
    console.log("cache_last_time_next shape:", results["cache_last_time_next"].dims);
    console.log("cache_last_channel_len_next value:", results["cache_last_channel_len_next"].data);
}

main().catch(console.error);
