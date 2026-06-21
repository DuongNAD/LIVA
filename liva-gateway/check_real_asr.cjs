const fs = require('fs');
const path = require('path');
const ort = require('onnxruntime-node');

const MODEL_DIR = 'e:/Project/LIVA/liva-gateway/models/nemotron-asr';
const WAV_FILE = 'e:/Project/LIVA/liva-ai-engine/venv/Lib/site-packages/fasr/asset/example/asr_example.wav';

const config = {
    numMels: 128, fftSize: 512, hopLength: 160, winLength: 400,
    preemph: 0.97, logEps: 5.96046448e-08, sampleRate: 16000, blankId: 13087,
    decoder: { hiddenSize: 640, numHiddenLayers: 2 },
    encoder: { hiddenSize: 1024, numHiddenLayers: 24 }
};

function hzToMel(hz) { return 2595 * Math.log10(1 + hz / 700); }
function melToHz(mel) { return 700 * (10 ** (mel / 2595) - 1); }
function computeHannWindow() { 
    const w = new Float32Array(400); 
    for (let i = 0; i < 400; i++) w[i] = 0.5 * (1 - Math.cos(2 * Math.PI * i / 399)); 
    return w; 
}

function computeMelFilterbank(useSlaney) {
    const n = 257; 
    const mlMin = hzToMel(0);
    const mlMax = hzToMel(8000);
    const mp = new Float32Array(130); 
    for (let i = 0; i < 130; i++) mp[i] = mlMin + i * (mlMax - mlMin) / 129;
    const bi = new Float32Array(130); 
    for (let i = 0; i < 130; i++) bi[i] = melToHz(mp[i]) * 512 / 16000;
    
    const f = []; 
    for (let m = 0; m < 128; m++) {
        const fl = new Float32Array(n);
        const l = bi[m];
        const c = bi[m+1];
        const r = bi[m+2];
        
        const leftHz = melToHz(mp[m]);
        const rightHz = melToHz(mp[m+2]);
        const enorm = useSlaney ? (2.0 / (rightHz - leftHz)) : 1.0;

        for (let k = 0; k < n; k++) { 
            if (k >= l && k <= c && c > l) {
                fl[k] = ((k-l)/(c-l)) * enorm; 
            } else if (k > c && k <= r && r > c) {
                fl[k] = ((r-k)/(r-c)) * enorm; 
            }
        }
        f.push(fl);
    } 
    return f;
}

function fft(frame) {
    const n = 512, re = new Float32Array(n), im = new Float32Array(n);
    for (let i = 0; i < frame.length && i < n; i++) re[i] = frame[i];
    for (let i = 1, j = 0; i < n; i++) { 
        let b = n >> 1; 
        while (j & b) { j ^= b; b >>= 1; } 
        j ^= b; 
        if (i < j) { [re[i],re[j]] = [re[j],re[i]]; [im[i],im[j]] = [im[j],im[i]]; } 
    }
    for (let len = 2; len <= n; len <<= 1) { 
        const h = len >> 1, a = -2*Math.PI/len, wr = Math.cos(a), wi = Math.sin(a);
        for (let i = 0; i < n; i += len) { 
            let cr = 1, ci = 0; 
            for (let j = 0; j < h; j++) { 
                const e = i+j, o = i+j+h;
                const tr = cr*re[o]-ci*im[o], ti = cr*im[o]+ci*re[o]; 
                re[o]=re[e]-tr; im[o]=im[e]-ti; re[e]+=tr; im[e]+=ti; 
                const nr = cr*wr-ci*wi; ci = cr*wi+ci*wr; cr = nr; 
            } 
        } 
    }
    const ps = new Float32Array(257); 
    for (let i = 0; i < 257; i++) ps[i] = re[i]*re[i]+im[i]*im[i]; 
    return ps;
}

function computeLogMelSpectrogram(samples, hannWindow, melFilterbank) {
    const numFrames = 65;
    const features = new Float32Array(numFrames * 128);
    const windowedFrame = new Float32Array(400);

    for (let f = 0; f < numFrames; f++) {
        const offset = f * 160;
        for (let i = 0; i < 400; i++) {
            windowedFrame[i] = samples[offset + i] * hannWindow[i];
        }
        const powerSpec = fft(windowedFrame);
        for (let m = 0; m < 128; m++) {
            let melEnergy = 0.0;
            const filter = melFilterbank[m];
            for (let k = 0; k < powerSpec.length; k++) {
                melEnergy += filter[k] * powerSpec[k];
            }
            features[f * 128 + m] = Math.log(melEnergy + config.logEps);
        }
    }
    return features;
}

function loadTokenizer(modelDir) {
    const tokenizerPath = path.join(modelDir, "tokenizer.json");
    const raw = fs.readFileSync(tokenizerPath, "utf-8");
    const parsed = JSON.parse(raw);
    const id2token = new Map();
    const model = parsed.model;
    if (model && Array.isArray(model.vocab)) {
        for (let i = 0; i < model.vocab.length; i++) {
            id2token.set(i, model.vocab[i][0]);
        }
    }
    return id2token;
}

function tokensToText(tokenIds, id2token) {
    const parts = [];
    const SPECIAL_TOKENS = new Set(["<blank>", "<unk>", "<s>", "</s>", "<pad>"]);
    for (const id of tokenIds) {
        if (id === config.blankId) continue;
        const token = id2token.get(id);
        if (!token || SPECIAL_TOKENS.has(token)) continue;
        if (token.startsWith("▁")) {
            const word = token.slice(1);
            if (word.length > 0) parts.push(" " + word);
        } else {
            parts.push(token);
        }
    }
    return parts.join("").trim();
}

async function runASROnAudio(useSlaney) {
    console.log(`\n==============================================`);
    console.log(`Running ASR with useSlaney=${useSlaney}...`);
    console.log(`==============================================`);
    
    // Load sessions
    const enc = await ort.InferenceSession.create(path.join(MODEL_DIR, 'encoder.onnx'), {executionProviders:['cpu']});
    const dec = await ort.InferenceSession.create(path.join(MODEL_DIR, 'decoder.onnx'), {executionProviders:['cpu']});
    const joi = await ort.InferenceSession.create(path.join(MODEL_DIR, 'joint.onnx'), {executionProviders:['cpu']});
    const id2token = loadTokenizer(MODEL_DIR);

    // Read WAV file (skip 44-byte header)
    const wavBuf = fs.readFileSync(WAV_FILE);
    const pcmData = wavBuf.subarray(44);
    const numSamples = pcmData.length / 2;
    const samples = new Float32Array(numSamples);
    for (let i = 0; i < numSamples; i++) {
        samples[i] = pcmData.readInt16LE(i * 2) / 32768.0;
    }
    console.log(`Loaded ${numSamples} speech samples.`);

    // Preemphasis
    const preemphed = new Float32Array(numSamples);
    let prev = 0;
    for (let i = 0; i < numSamples; i++) {
        preemphed[i] = samples[i] - config.preemph * prev;
        prev = samples[i];
    }

    const hannWindow = computeHannWindow();
    const melFilterbank = computeMelFilterbank(useSlaney);

    // Initial state
    let cacheLastChannel = new Float32Array(24 * 56 * 1024).fill(0);
    let cacheLastTime = new Float32Array(24 * 1024 * 8).fill(0);
    let cacheLastChannelLen = new BigInt64Array([0n]);
    
    let decoderHiddenState = new Float32Array(2 * 1 * 640).fill(0);
    let decoderCellState = new Float32Array(2 * 1 * 640).fill(0);
    let lastDecoderToken = config.blankId;
    
    let accumulatedTokenIds = [];
    let residualSamples = new Float32Array(0);

    // Feed loop
    let sampleOffset = 0;
    const chunkSize = 2048; // same as client
    
    while (sampleOffset < numSamples) {
        const chunkEnd = Math.min(sampleOffset + chunkSize, numSamples);
        const chunk = preemphed.subarray(sampleOffset, chunkEnd);
        sampleOffset = chunkEnd;

        const combined = new Float32Array(residualSamples.length + chunk.length);
        combined.set(residualSamples, 0);
        combined.set(chunk, residualSamples.length);
        residualSamples = combined;

        while (residualSamples.length >= 10640) {
            const slice = residualSamples.subarray(0, 10640);
            const features = computeLogMelSpectrogram(slice, hannWindow, melFilterbank);

            // Run encoder
            const r = await enc.run({
                audio_signal: new ort.Tensor('float32', features, [1, 65, 128]),
                length: new ort.Tensor('int64', BigInt64Array.from([65n]), [1]),
                cache_last_channel: new ort.Tensor('float32', cacheLastChannel, [1, 24, 56, 1024]),
                cache_last_time: new ort.Tensor('float32', cacheLastTime, [1, 24, 1024, 8]),
                cache_last_channel_len: new ort.Tensor('int64', cacheLastChannelLen, [1]),
                lang_id: new ort.Tensor('int64', BigInt64Array.from([33n]), [1]) // vi
            });

            // Update caches
            cacheLastChannel = new Float32Array(r['cache_last_channel_next'].data);
            cacheLastTime = new Float32Array(r['cache_last_time_next'].data);
            cacheLastChannelLen = new BigInt64Array(r['cache_last_channel_len_next'].data);

            const outLen = Number(r['encoded_lengths'].data[0]);
            const ed = r['outputs'].data;

            // Decode
            for (let t = 0; t < outLen; t++) {
                const fd = new Float32Array(1024);
                for (let d = 0; d < 1024; d++) fd[d] = ed[t*1024+d];
                const ef = new ort.Tensor('float32', fd, [1,1,1024]);

                let stepsThisFrame = 0;
                while (stepsThisFrame < 10) {
                    // Decoder run
                    const decResult = await dec.run({
                        targets: new ort.Tensor('int64', BigInt64Array.from([BigInt(lastDecoderToken)]), [1,1]),
                        h_in: new ort.Tensor('float32', decoderHiddenState, [2,1,640]),
                        c_in: new ort.Tensor('float32', decoderCellState, [2,1,640])
                    });
                    
                    decoderHiddenState = new Float32Array(decResult['h_out'].data);
                    decoderCellState = new Float32Array(decResult['c_out'].data);
                    const decOut = decResult['decoder_output'];
                    
                    const rd = new ort.Tensor('float32', decOut.data, [1,1,640]);
                    
                    // Joiner run
                    const jr = await joi.run({ encoder_output: ef, decoder_output: rd });
                    const lg = jr['joint_output'].data;
                    
                    let maxIdx = 0;
                    let maxVal = lg[0];
                    for (let i = 1; i < lg.length; i++) {
                        if (lg[i] > maxVal) { maxVal = lg[i]; maxIdx = i; }
                    }
                    
                    stepsThisFrame++;
                    if (maxIdx === config.blankId) {
                        break;
                    }
                    
                    accumulatedTokenIds.push(maxIdx);
                    lastDecoderToken = maxIdx;
                }
            }
            residualSamples = residualSamples.slice(8960);
        }
    }

    const text = tokensToText(accumulatedTokenIds, id2token);
    console.log(`Tokens count: ${accumulatedTokenIds.length}`);
    console.log(`ASR Decoded Text: "${text}"`);

    await enc.release(); await dec.release(); await joi.release();
}

async function run() {
    await runASROnAudio(true);
    await runASROnAudio(false);
}

run().catch(console.error);
