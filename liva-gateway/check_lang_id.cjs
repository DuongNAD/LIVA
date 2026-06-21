// Check what lang_id values produce output from encoder
const ort = require('onnxruntime-node');
const fs = require('fs');
const path = require('path');

const MODEL_DIR = 'e:/Project/LIVA/liva-gateway/models/nemotron-asr';

async function main() {
    console.log('Loading encoder...');
    const encoder = await ort.InferenceSession.create(
        path.join(MODEL_DIR, 'encoder.onnx'),
        { executionProviders: ['cpu'] }
    );
    
    console.log('Inputs:', encoder.inputNames);
    console.log('Outputs:', encoder.outputNames);
    
    // Check model metadata
    try {
        const modelMeta = encoder.handler?.metadata;
        if (modelMeta) {
            console.log('\nModel metadata:');
            for (const [k, v] of Object.entries(modelMeta)) {
                console.log(`  ${k}: ${v}`);
            }
        }
    } catch(e) {}

    // Generate a simple test audio (1 second of 440Hz sine wave)
    const sampleRate = 16000;
    const numFrames = 65;
    const numMels = 128;
    
    // Create random-ish features that look like mel spectrogram
    const features = new Float32Array(numFrames * numMels);
    for (let f = 0; f < numFrames; f++) {
        for (let m = 0; m < numMels; m++) {
            features[f * numMels + m] = -10 + Math.random() * 5; // typical log-mel range
        }
    }
    
    const featureTensor = new ort.Tensor('float32', features, [1, numFrames, numMels]);
    const lengthTensor = new ort.Tensor('int64', BigInt64Array.from([BigInt(numFrames)]), [1]);
    
    // Cache tensors (zeros)
    const channelElements = 24 * 56 * 1024;
    const timeElements = 24 * 1024 * 8;
    const cacheChannel = new ort.Tensor('float32', new Float32Array(channelElements), [1, 24, 56, 1024]);
    const cacheTime = new ort.Tensor('float32', new Float32Array(timeElements), [1, 24, 1024, 8]);
    const cacheChannelLen = new ort.Tensor('int64', new BigInt64Array([0n]), [1]);
    
    // Try different lang_id values
    console.log('\n--- Testing lang_id values ---');
    
    const decoder = await ort.InferenceSession.create(
        path.join(MODEL_DIR, 'decoder.onnx'), 
        { executionProviders: ['cpu'] }
    );
    const joiner = await ort.InferenceSession.create(
        path.join(MODEL_DIR, 'joint.onnx'),
        { executionProviders: ['cpu'] }
    );
    
    console.log('Decoder inputs:', decoder.inputNames);
    console.log('Decoder outputs:', decoder.outputNames);
    console.log('Joiner inputs:', joiner.inputNames);
    console.log('Joiner outputs:', joiner.outputNames);
    
    // Test lang_id = 33 (current Vietnamese guess)
    for (const langId of [0, 33, 101]) {
        try {
            const langIdTensor = new ort.Tensor('int64', BigInt64Array.from([BigInt(langId)]), [1]);
            
            const result = await encoder.run({
                audio_signal: featureTensor,
                length: lengthTensor,
                cache_last_channel: cacheChannel,
                cache_last_time: cacheTime,
                cache_last_channel_len: cacheChannelLen,
                lang_id: langIdTensor
            });
            
            const outLen = result['encoded_lengths'] 
                ? Number(result['encoded_lengths'].data[0]) 
                : result['outputs'].dims[1];
            
            console.log(`lang_id=${langId}: outLen=${outLen}, output_shape=${result['outputs'].dims}`);
        } catch(e) {
            console.log(`lang_id=${langId}: ERROR - ${e.message}`);
        }
    }
    
    await encoder.release();
    await decoder.release();
    await joiner.release();
}

main().catch(console.error);
