// End-to-end test: sine wave → mel → encoder → decoder → joiner → tokens
const ort = require('onnxruntime-node');
const path = require('path');
const fs = require('fs');
const MODEL_DIR = 'e:/Project/LIVA/liva-gateway/models/nemotron-asr';

const config = {
    numMels: 128, fftSize: 512, hopLength: 160, winLength: 400,
    preemph: 0.97, logEps: 5.96046448e-08,
    subsamplingFactor: 8, leftContext: 56, convContext: 8,
    sampleRate: 16000, blankId: 13087, maxSymbolsPerStep: 10,
    encoder: { hiddenSize: 1024, numHiddenLayers: 24 },
    decoder: { hiddenSize: 640, numHiddenLayers: 2 },
};

function hzToMel(hz) { return 2595.0 * Math.log10(1.0 + hz / 700.0); }
function melToHz(mel) { return 700.0 * (Math.pow(10, mel / 2595.0) - 1.0); }

function computeHannWindow() {
    const w = new Float32Array(config.winLength);
    for (let i = 0; i < config.winLength; i++)
        w[i] = 0.5 * (1.0 - Math.cos((2.0 * Math.PI * i) / (config.winLength - 1)));
    return w;
}

function computeMelFilterbank() {
    const numBins = config.fftSize / 2 + 1;
    const melMin = hzToMel(0), melMax = hzToMel(8000);
    const melPoints = new Float32Array(config.numMels + 2);
    for (let i = 0; i < config.numMels + 2; i++)
        melPoints[i] = melMin + (i * (melMax - melMin)) / (config.numMels + 1);
    const binIndices = new Float32Array(config.numMels + 2);
    for (let i = 0; i < config.numMels + 2; i++)
        binIndices[i] = (melToHz(melPoints[i]) * config.fftSize) / config.sampleRate;
    const filters = [];
    for (let m = 0; m < config.numMels; m++) {
        const filter = new Float32Array(numBins);
        const left = binIndices[m], center = binIndices[m + 1], right = binIndices[m + 2];
        for (let k = 0; k < numBins; k++) {
            if (k >= left && k <= center && center > left) filter[k] = (k - left) / (center - left);
            else if (k > center && k <= right && right > center) filter[k] = (right - k) / (right - center);
        }
        filters.push(filter);
    }
    return filters;
}

function computeFFTMagnitudeSq(frame) {
    const n = config.fftSize;
    const real = new Float32Array(n), imag = new Float32Array(n);
    for (let i = 0; i < frame.length && i < n; i++) real[i] = frame[i];
    for (let i = 1, j = 0; i < n; i++) {
        let bit = n >> 1;
        while (j & bit) { j ^= bit; bit >>= 1; }
        j ^= bit;
        if (i < j) { [real[i], real[j]] = [real[j], real[i]]; [imag[i], imag[j]] = [imag[j], imag[i]]; }
    }
    for (let len = 2; len <= n; len <<= 1) {
        const halfLen = len >> 1, angle = (-2.0 * Math.PI) / len;
        const wR = Math.cos(angle), wI = Math.sin(angle);
        for (let i = 0; i < n; i += len) {
            let curR = 1.0, curI = 0.0;
            for (let j = 0; j < halfLen; j++) {
                const ei = i + j, oi = i + j + halfLen;
                const tR = curR * real[oi] - curI * imag[oi], tI = curR * imag[oi] + curI * real[oi];
                real[oi] = real[ei] - tR; imag[oi] = imag[ei] - tI;
                real[ei] += tR; imag[ei] += tI;
                const nextR = curR * wR - curI * wI; curI = curR * wI + curI * wR; curR = nextR;
            }
        }
    }
    const numBins = n / 2 + 1, ps = new Float32Array(numBins);
    for (let i = 0; i < numBins; i++) ps[i] = real[i] * real[i] + imag[i] * imag[i];
    return ps;
}

async function main() {
    // Generate test audio: 1 second of speech-like signal
    const sr = 16000;
    // Mix of frequencies to simulate speech-like content
    const totalSamples = 10640; // exactly what encoder needs
    const raw = new Float32Array(totalSamples);
    for (let i = 0; i < totalSamples; i++) {
        const t = i / sr;
        // Mix of harmonics (200Hz fundamental + overtones)
        raw[i] = 0.3 * Math.sin(2 * Math.PI * 200 * t) +
                 0.2 * Math.sin(2 * Math.PI * 400 * t) +
                 0.15 * Math.sin(2 * Math.PI * 600 * t) +
                 0.1 * Math.sin(2 * Math.PI * 1000 * t) +
                 0.05 * (Math.random() - 0.5); // noise
    }

    // Scale to Int16 range (like current code does)
    const scaled = new Float32Array(totalSamples);
    let prevSample = 0;
    for (let i = 0; i < totalSamples; i++) {
        const val = raw[i] * 32768.0;
        scaled[i] = val - config.preemph * prevSample;
        prevSample = val;
    }

    // Compute mel spectrogram
    const hannWindow = computeHannWindow();
    const melFilterbank = computeMelFilterbank();
    const numFrames = 65;
    const features = new Float32Array(numFrames * config.numMels);
    const windowedFrame = new Float32Array(config.winLength);

    for (let f = 0; f < numFrames; f++) {
        const offset = f * config.hopLength;
        for (let i = 0; i < config.winLength; i++) windowedFrame[i] = scaled[offset + i] * hannWindow[i];
        const powerSpec = computeFFTMagnitudeSq(windowedFrame);
        for (let m = 0; m < config.numMels; m++) {
            let melEnergy = 0.0;
            for (let k = 0; k < powerSpec.length; k++) melEnergy += melFilterbank[m][k] * powerSpec[k];
            features[f * config.numMels + m] = Math.log(melEnergy + config.logEps);
        }
    }

    // Print mel feature stats
    let minMel = Infinity, maxMel = -Infinity, sumMel = 0;
    for (let i = 0; i < features.length; i++) {
        minMel = Math.min(minMel, features[i]); maxMel = Math.max(maxMel, features[i]); sumMel += features[i];
    }
    console.log(`\nMel features range: [${minMel.toFixed(2)}, ${maxMel.toFixed(2)}] mean=${(sumMel/features.length).toFixed(2)}`);
    console.log(`First 10 mel values: ${Array.from(features.subarray(0, 10)).map(v => v.toFixed(2)).join(', ')}`);

    // Load ONNX models
    console.log('\nLoading ONNX models...');
    const encoder = await ort.InferenceSession.create(path.join(MODEL_DIR, 'encoder.onnx'), {executionProviders:['cpu']});
    const decoder = await ort.InferenceSession.create(path.join(MODEL_DIR, 'decoder.onnx'), {executionProviders:['cpu']});
    const joiner = await ort.InferenceSession.create(path.join(MODEL_DIR, 'joint.onnx'), {executionProviders:['cpu']});

    // Run encoder
    const featureTensor = new ort.Tensor('float32', features, [1, numFrames, config.numMels]);
    const lengthTensor = new ort.Tensor('int64', BigInt64Array.from([BigInt(numFrames)]), [1]);
    const cacheChannel = new ort.Tensor('float32', new Float32Array(24 * 56 * 1024), [1, 24, 56, 1024]);
    const cacheTime = new ort.Tensor('float32', new Float32Array(24 * 1024 * 8), [1, 24, 1024, 8]);
    const cacheChannelLen = new ort.Tensor('int64', new BigInt64Array([0n]), [1]);
    const langId = new ort.Tensor('int64', BigInt64Array.from([33n]), [1]);

    const encResult = await encoder.run({
        audio_signal: featureTensor, length: lengthTensor,
        cache_last_channel: cacheChannel, cache_last_time: cacheTime,
        cache_last_channel_len: cacheChannelLen, lang_id: langId
    });

    const encoderOut = encResult['outputs'];
    const encoderOutLen = Number(encResult['encoded_lengths'].data[0]);
    console.log(`\nEncoder output: shape=${encoderOut.dims}, outLen=${encoderOutLen}`);

    // Run decoder with blank token
    const decoderInput = new ort.Tensor('int64', BigInt64Array.from([BigInt(config.blankId)]), [1, 1]);
    const hIn = new ort.Tensor('float32', new Float32Array(2 * 640), [2, 1, 640]);
    const cIn = new ort.Tensor('float32', new Float32Array(2 * 640), [2, 1, 640]);
    const decResult = await decoder.run({ targets: decoderInput, h_in: hIn, c_in: cIn });
    let decoderOut = decResult['decoder_output'];
    console.log(`Decoder output: shape=${decoderOut.dims}`);

    // Run joiner for each encoder frame
    const encData = encoderOut.data;
    const featureDim = encoderOut.dims[2];
    let totalTokens = 0;
    
    for (let t = 0; t < encoderOutLen; t++) {
        const frameData = new Float32Array(featureDim);
        for (let d = 0; d < featureDim; d++) frameData[d] = encData[t * featureDim + d];
        const encoderFrame = new ort.Tensor('float32', frameData, [1, 1, featureDim]);
        
        const reshapedDecOut = new ort.Tensor('float32', decoderOut.data, [1, 1, 640]);
        const jointResult = await joiner.run({ encoder_output: encoderFrame, decoder_output: reshapedDecOut });
        const logits = jointResult['joint_output'].data;
        
        // Find top-5 logits
        const indexed = Array.from(logits).map((v, i) => ({v, i})).sort((a, b) => b.v - a.v);
        if (t === 0) {
            console.log(`\nFrame 0 joiner logits top-5:`);
            for (let i = 0; i < 5; i++) {
                console.log(`  idx=${indexed[i].i} logit=${indexed[i].v.toFixed(4)}`);
            }
            console.log(`  blank_id=${config.blankId} blank_logit=${logits[config.blankId]?.toFixed(4)}`);
        }
        
        // Argmax
        const tokenId = indexed[0].i;
        if (tokenId !== config.blankId) {
            totalTokens++;
            console.log(`  Frame ${t}: token=${tokenId} (non-blank!)`);
        }
    }
    
    console.log(`\nTotal non-blank tokens: ${totalTokens} out of ${encoderOutLen} frames`);

    // Load tokenizer to decode
    const tokRaw = JSON.parse(fs.readFileSync(path.join(MODEL_DIR, 'tokenizer.json'), 'utf-8'));
    const vocabArr = tokRaw.model?.vocab || [];
    if (totalTokens === 0) {
        console.log('\n⚠️ All frames returned blank token — model not recognizing anything');
        console.log('This could mean:');
        console.log('  1. Mel spectrogram values not in expected range');
        console.log('  2. lang_id mapping incorrect');
        console.log('  3. Audio signal needs different normalization');
    }

    await encoder.release(); await decoder.release(); await joiner.release();
}

main().catch(console.error);
