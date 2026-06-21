const ort = require('onnxruntime-node');
const path = require('path');
const fs = require('fs');

const DEFAULT_CONFIG = {
    vocabSize: 13088,
    numMels: 128,
    fftSize: 512,
    hopLength: 160,
    winLength: 400,
    preemph: 0.97,
    logEps: 5.96046448e-08,
    subsamplingFactor: 8,
    leftContext: 56,
    convContext: 8,
    preEncodeCacheSize: 9,
    sampleRate: 16000,
    chunkSamples: 8960,
    blankId: 13087,
    maxSymbolsPerStep: 10,
    encoder: { filename: "encoder.onnx", hiddenSize: 1024, numHiddenLayers: 24 },
    decoder: { filename: "decoder.onnx", hiddenSize: 640, numHiddenLayers: 2 },
    joiner: { filename: "joint.onnx" },
};

let config = DEFAULT_CONFIG;
let languageId = 33; // Vietnamese

function hzToMel(hz) {
    return 2595.0 * Math.log10(1.0 + hz / 700.0);
}

function melToHz(mel) {
    return 700.0 * (Math.pow(10.0, mel / 2595.0) - 1.0);
}

function computeMelFilterbank() {
    const numBins = config.fftSize / 2 + 1;
    const melMin = hzToMel(0);
    const melMax = hzToMel(8000);

    const melPoints = new Float32Array(config.numMels + 2);
    for (let i = 0; i < config.numMels + 2; i++) {
        melPoints[i] = melMin + (i * (melMax - melMin)) / (config.numMels + 1);
    }

    const binIndices = new Float32Array(config.numMels + 2);
    for (let i = 0; i < config.numMels + 2; i++) {
        const hz = melToHz(melPoints[i]);
        binIndices[i] = (hz * config.fftSize) / config.sampleRate;
    }

    const filters = [];
    for (let m = 0; m < config.numMels; m++) {
        const filter = new Float32Array(numBins);
        const left = binIndices[m];
        const center = binIndices[m + 1];
        const right = binIndices[m + 2];

        for (let k = 0; k < numBins; k++) {
            if (k >= left && k <= center && center > left) {
                filter[k] = (k - left) / (center - left);
            } else if (k > center && k <= right && right > center) {
                filter[k] = (right - k) / (right - center);
            }
        }
        filters.push(filter);
    }
    return filters;
}

function computeHannWindow() {
    const window = new Float32Array(config.winLength);
    for (let i = 0; i < config.winLength; i++) {
        window[i] = 0.5 * (1.0 - Math.cos((2.0 * Math.PI * i) / (config.winLength - 1)));
    }
    return window;
}

function computeFFTMagnitudeSq(frame) {
    const n = config.fftSize;
    const real = new Float32Array(n);
    const imag = new Float32Array(n);
    for (let i = 0; i < frame.length && i < n; i++) {
        real[i] = frame[i];
    }

    for (let i = 1, j = 0; i < n; i++) {
        let bit = n >> 1;
        while (j & bit) {
            j ^= bit;
            bit >>= 1;
        }
        j ^= bit;
        if (i < j) {
            const tmpR = real[i]; real[i] = real[j]; real[j] = tmpR;
            const tmpI = imag[i]; imag[i] = imag[j]; imag[j] = tmpI;
        }
    }

    for (let len = 2; len <= n; len <<= 1) {
        const halfLen = len >> 1;
        const angle = (-2.0 * Math.PI) / len;
        const wR = Math.cos(angle);
        const wI = Math.sin(angle);

        for (let i = 0; i < n; i += len) {
            let curR = 1.0;
            let curI = 0.0;
            for (let j = 0; j < halfLen; j++) {
                const evenIdx = i + j;
                const oddIdx = i + j + halfLen;
                const tR = curR * real[oddIdx] - curI * imag[oddIdx];
                const tI = curR * imag[oddIdx] + curI * real[oddIdx];
                real[oddIdx] = real[evenIdx] - tR;
                imag[oddIdx] = imag[evenIdx] - tI;
                real[evenIdx] += tR;
                imag[evenIdx] += tI;
                const nextR = curR * wR - curI * wI;
                curI = curR * wI + curI * wR;
                curR = nextR;
            }
        }
    }

    const numBins = n / 2 + 1;
    const powerSpec = new Float32Array(numBins);
    for (let i = 0; i < numBins; i++) {
        powerSpec[i] = real[i] * real[i] + imag[i] * imag[i];
    }
    return powerSpec;
}

const hannWindow = computeHannWindow();
const melFilterbank = computeMelFilterbank();

function computeLogMelSpectrogram(samples) {
    const numFrames = 65;
    const features = new Float32Array(numFrames * config.numMels);
    const windowedFrame = new Float32Array(config.winLength);

    for (let f = 0; f < numFrames; f++) {
        const offset = f * config.hopLength;

        for (let i = 0; i < config.winLength; i++) {
            windowedFrame[i] = samples[offset + i] * hannWindow[i];
        }

        const powerSpec = computeFFTMagnitudeSq(windowedFrame);

        for (let m = 0; m < config.numMels; m++) {
            let melEnergy = 0.0;
            const filter = melFilterbank[m];
            for (let k = 0; k < powerSpec.length; k++) {
                melEnergy += filter[k] * powerSpec[k];
            }
            features[f * config.numMels + m] = Math.log(melEnergy + config.logEps);
        }
    }

    return features;
}

async function main() {
    const modelDir = 'e:\\Project\\LIVA\\liva-gateway\\models\\nemotron-asr';
    const encoderPath = path.join(modelDir, 'encoder.onnx');
    const decoderPath = path.join(modelDir, 'decoder.onnx');
    const joinerPath = path.join(modelDir, 'joint.onnx');

    console.log("Loading sessions...");
    const encoderSession = await ort.InferenceSession.create(encoderPath);
    const decoderSession = await ort.InferenceSession.create(decoderPath);
    const joinerSession = await ort.InferenceSession.create(joinerPath);

    // Initial state
    const channelElements = config.encoder.numHiddenLayers * 1 * config.encoder.hiddenSize * config.leftContext;
    let cacheLastChannel = new Float32Array(channelElements).fill(0);
    const timeElements = config.encoder.numHiddenLayers * 1 * config.encoder.hiddenSize * config.convContext;
    let cacheLastTime = new Float32Array(timeElements).fill(0);
    let cacheLastChannelLen = new BigInt64Array([0n]);

    const stateElements = config.decoder.numHiddenLayers * 1 * config.decoder.hiddenSize;
    let decoderHiddenState = new Float32Array(stateElements).fill(0);
    let decoderCellState = new Float32Array(stateElements).fill(0);
    let lastDecoderToken = config.blankId;

    // Process single dummy chunk (all zeroes)
    console.log("Generating 10640 samples...");
    const samples = new Float32Array(10640).fill(0);

    console.log("Computing Mel Spectrogram...");
    const features = computeLogMelSpectrogram(samples);

    console.log("Running Encoder...");
    const featureTensor = new ort.Tensor("float32", features, [1, 65, config.numMels]);
    const lengthTensor = new ort.Tensor("int64", BigInt64Array.from([65n]), [1]);
    const cacheChannelTensor = new ort.Tensor("float32", cacheLastChannel, [1, config.encoder.numHiddenLayers, config.leftContext, config.encoder.hiddenSize]);
    const cacheTimeTensor = new ort.Tensor("float32", cacheLastTime, [1, config.encoder.numHiddenLayers, config.encoder.hiddenSize, config.convContext]);
    const cacheChannelLenTensor = new ort.Tensor("int64", cacheLastChannelLen, [1]);
    const langIdTensor = new ort.Tensor("int64", BigInt64Array.from([BigInt(languageId)]), [1]);

    const encResults = await encoderSession.run({
        audio_signal: featureTensor,
        length: lengthTensor,
        cache_last_channel: cacheChannelTensor,
        cache_last_time: cacheTimeTensor,
        cache_last_channel_len: cacheChannelLenTensor,
        lang_id: langIdTensor
    });

    const encoderOut = encResults["outputs"];
    const outLenTensor = encResults["encoded_lengths"];
    const encoderOutLen = outLenTensor ? Number(outLenTensor.data[0]) : encoderOut.dims[1];

    console.log(`Encoder output dims: ${encoderOut.dims}, length: ${encoderOutLen}`);

    // Decode loop
    console.log("Running Decoder + Joiner loop...");
    const encoderData = encoderOut.data;
    const featureDim = encoderOut.dims[2];

    // Decoder initial step
    const targets = new ort.Tensor("int64", BigInt64Array.from([BigInt(lastDecoderToken)]), [1, 1]);
    const hInTensor = new ort.Tensor("float32", decoderHiddenState, [config.decoder.numHiddenLayers, 1, config.decoder.hiddenSize]);
    const cInTensor = new ort.Tensor("float32", decoderCellState, [config.decoder.numHiddenLayers, 1, config.decoder.hiddenSize]);

    const decResults = await decoderSession.run({ targets, h_in: hInTensor, c_in: cInTensor });
    let decoderOut = decResults["decoder_output"];
    
    // Save state
    decoderHiddenState = new Float32Array(decResults["h_out"].data);
    decoderCellState = new Float32Array(decResults["c_out"].data);

    for (let t = 0; t < encoderOutLen; t++) {
        const frameStart = t * featureDim;
        const frameData = new Float32Array(featureDim);
        for (let d = 0; d < featureDim; d++) {
            frameData[d] = encoderData[frameStart + d];
        }
        const encoderFrame = new ort.Tensor("float32", frameData, [1, 1, featureDim]);

        let stepsThisFrame = 0;
        while (stepsThisFrame < config.maxSymbolsPerStep) {
            // Apply reshape fix
            const reshapedDecoderOut = new ort.Tensor(
                "float32",
                decoderOut.data,
                [1, 1, config.decoder.hiddenSize]
            );

            const jointResults = await joinerSession.run({
                encoder_output: encoderFrame,
                decoder_output: reshapedDecoderOut
            });

            const logits = jointResults["joint_output"].data;
            let maxIdx = 0;
            let maxVal = logits[0];
            for (let i = 1; i < logits.length; i++) {
                if (logits[i] > maxVal) {
                    maxVal = logits[i];
                    maxIdx = i;
                }
            }

            stepsThisFrame++;
            if (maxIdx === config.blankId) {
                break;
            }

            console.log(`Emitted Token ID: ${maxIdx}`);
            lastDecoderToken = maxIdx;

            const targetsNext = new ort.Tensor("int64", BigInt64Array.from([BigInt(lastDecoderToken)]), [1, 1]);
            const hIn = new ort.Tensor("float32", decoderHiddenState, [config.decoder.numHiddenLayers, 1, config.decoder.hiddenSize]);
            const cIn = new ort.Tensor("float32", decoderCellState, [config.decoder.numHiddenLayers, 1, config.decoder.hiddenSize]);

            const nextDecResults = await decoderSession.run({ targets: targetsNext, h_in: hIn, c_in: cIn });
            decoderOut = nextDecResults["decoder_output"];
            decoderHiddenState = new Float32Array(nextDecResults["h_out"].data);
            decoderCellState = new Float32Array(nextDecResults["c_out"].data);
        }
    }

    console.log("End-to-end pipeline completed successfully!");
}

main().catch(console.error);
