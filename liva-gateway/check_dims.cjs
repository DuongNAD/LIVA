// Inspect encoder ONNX model input shapes
const ort = require('onnxruntime-node');
const path = require('path');
const MODEL_DIR = 'e:/Project/LIVA/liva-gateway/models/nemotron-asr';

async function main() {
    const encoder = await ort.InferenceSession.create(
        path.join(MODEL_DIR, 'encoder.onnx'),
        { executionProviders: ['cpu'] }
    );

    // Try to get input shape metadata 
    console.log('\n=== ENCODER INPUT/OUTPUT SHAPES ===');
    
    // Test with various dim orderings to find which works
    const numLayers = 24, hiddenSize = 1024, leftContext = 56, convContext = 8;
    const numMels = 128, numFrames = 65;
    
    // Create test inputs
    const features = new Float32Array(numFrames * numMels);
    for (let i = 0; i < features.length; i++) features[i] = -5 + Math.random() * 3;
    
    const featureTensor = new ort.Tensor('float32', features, [1, numFrames, numMels]);
    const lengthTensor = new ort.Tensor('int64', BigInt64Array.from([BigInt(numFrames)]), [1]);
    const langIdTensor = new ort.Tensor('int64', BigInt64Array.from([33n]), [1]);
    const cacheChannelLen = new ort.Tensor('int64', new BigInt64Array([0n]), [1]);
    
    // Test different cache dimension orderings
    const channelOrders = [
        { name: '[1, 24, 56, 1024]', dims: [1, 24, 56, 1024] },
        { name: '[1, 24, 1024, 56]', dims: [1, 24, 1024, 56] },
    ];
    const timeOrders = [
        { name: '[1, 24, 1024, 8]', dims: [1, 24, 1024, 8] },
        { name: '[1, 24, 8, 1024]', dims: [1, 24, 8, 1024] },
    ];
    
    for (const ch of channelOrders) {
        for (const tm of timeOrders) {
            try {
                const chSize = ch.dims.reduce((a,b) => a*b, 1);
                const tmSize = tm.dims.reduce((a,b) => a*b, 1);
                const cacheCh = new ort.Tensor('float32', new Float32Array(chSize), ch.dims);
                const cacheTm = new ort.Tensor('float32', new Float32Array(tmSize), tm.dims);
                
                const result = await encoder.run({
                    audio_signal: featureTensor,
                    length: lengthTensor,
                    cache_last_channel: cacheCh,
                    cache_last_time: cacheTm,
                    cache_last_channel_len: cacheChannelLen,
                    lang_id: langIdTensor,
                });
                
                const outLen = Number(result['encoded_lengths'].data[0]);
                
                // Check output cache shapes
                const nextCh = result['cache_last_channel_next'];
                const nextTm = result['cache_last_time_next'];
                
                console.log(`\n✅ WORKS: channel=${ch.name} time=${tm.name}`);
                console.log(`   output_shape: ${result['outputs'].dims}`);
                console.log(`   outLen: ${outLen}`);
                console.log(`   cache_channel_next_shape: ${nextCh.dims}`);
                console.log(`   cache_time_next_shape: ${nextTm.dims}`);
                
                // Check encoder output values
                const outData = result['outputs'].data;
                let min = Infinity, max = -Infinity, sum = 0;
                for (let i = 0; i < outData.length; i++) {
                    min = Math.min(min, outData[i]);
                    max = Math.max(max, outData[i]);
                    sum += outData[i];
                }
                console.log(`   encoder_out range: [${min.toFixed(4)}, ${max.toFixed(4)}], mean=${(sum/outData.length).toFixed(4)}`);
                
            } catch(e) {
                console.log(`❌ FAIL: channel=${ch.name} time=${tm.name} — ${e.message.substring(0, 100)}`);
            }
        }
    }
    
    await encoder.release();
}

main().catch(console.error);
