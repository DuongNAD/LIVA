// Quick test with Slaney normalization
const ort = require('onnxruntime-node');
const path = require('path');
const MODEL_DIR = 'e:/Project/LIVA/liva-gateway/models/nemotron-asr';
const config = {
    numMels: 128, fftSize: 512, hopLength: 160, winLength: 400,
    preemph: 0.97, logEps: 5.96046448e-08, sampleRate: 16000, blankId: 13087,
    decoder: { hiddenSize: 640, numHiddenLayers: 2 },
};
function hzToMel(hz) { return 2595 * Math.log10(1 + hz / 700); }
function melToHz(mel) { return 700 * (10 ** (mel / 2595) - 1); }
function computeHannWindow() { const w = new Float32Array(400); for (let i = 0; i < 400; i++) w[i] = 0.5 * (1 - Math.cos(2 * Math.PI * i / 399)); return w; }

function computeMelFilterbank() {
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
        
        // Slaney normalization factor
        const leftHz = melToHz(mp[m]);
        const rightHz = melToHz(mp[m+2]);
        const enorm = 2.0 / (rightHz - leftHz);

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
    for (let i = 1, j = 0; i < n; i++) { let b = n >> 1; while (j & b) { j ^= b; b >>= 1; } j ^= b; if (i < j) { [re[i],re[j]] = [re[j],re[i]]; [im[i],im[j]] = [im[j],im[i]]; } }
    for (let len = 2; len <= n; len <<= 1) { const h = len >> 1, a = -2*Math.PI/len, wr = Math.cos(a), wi = Math.sin(a);
        for (let i = 0; i < n; i += len) { let cr = 1, ci = 0; for (let j = 0; j < h; j++) { const e = i+j, o = i+j+h;
            const tr = cr*re[o]-ci*im[o], ti = cr*im[o]+ci*re[o]; re[o]=re[e]-tr; im[o]=im[e]-ti; re[e]+=tr; im[e]+=ti; const nr = cr*wr-ci*wi; ci = cr*wi+ci*wr; cr = nr; } } }
    const ps = new Float32Array(257); for (let i = 0; i < 257; i++) ps[i] = re[i]*re[i]+im[i]*im[i]; return ps;
}

async function main() {
    const sr = 16000, N = 10640;
    const raw = new Float32Array(N);
    // Create some speech-like signals (combination of harmonics)
    for (let i = 0; i < N; i++) { 
        const t = i/sr; 
        raw[i] = 0.3*Math.sin(2*Math.PI*200*t) + 0.2*Math.sin(2*Math.PI*400*t) + 0.15*Math.sin(2*Math.PI*600*t) + 0.1*Math.sin(2*Math.PI*1000*t) + 0.05*(Math.random()-0.5); 
    }

    const preemphed = new Float32Array(N); let prev = 0;
    for (let i = 0; i < N; i++) { preemphed[i] = raw[i] - 0.97 * prev; prev = raw[i]; }

    const hw = computeHannWindow(), mf = computeMelFilterbank();
    const feat = new Float32Array(65 * 128), wf = new Float32Array(400);
    for (let f = 0; f < 65; f++) { const o = f * 160;
        for (let i = 0; i < 400; i++) wf[i] = preemphed[o+i] * hw[i];
        const ps = fft(wf);
        for (let m = 0; m < 128; m++) { 
            let e = 0; 
            for (let k = 0; k < 257; k++) e += mf[m][k]*ps[k]; 
            feat[f*128+m] = Math.log(e + 5.96046448e-08); 
        }
    }

    let mn = Infinity, mx = -Infinity, sm = 0;
    for (let i = 0; i < feat.length; i++) { mn = Math.min(mn, feat[i]); mx = Math.max(mx, feat[i]); sm += feat[i]; }
    console.log(`Mel (Slaney normalized): [${mn.toFixed(2)}, ${mx.toFixed(2)}] mean=${(sm/feat.length).toFixed(2)}`);

    const enc = await ort.InferenceSession.create(path.join(MODEL_DIR, 'encoder.onnx'), {executionProviders:['cpu']});
    const dec = await ort.InferenceSession.create(path.join(MODEL_DIR, 'decoder.onnx'), {executionProviders:['cpu']});
    const joi = await ort.InferenceSession.create(path.join(MODEL_DIR, 'joint.onnx'), {executionProviders:['cpu']});

    const r = await enc.run({
        audio_signal: new ort.Tensor('float32', feat, [1, 65, 128]),
        length: new ort.Tensor('int64', BigInt64Array.from([65n]), [1]),
        cache_last_channel: new ort.Tensor('float32', new Float32Array(24*56*1024), [1,24,56,1024]),
        cache_last_time: new ort.Tensor('float32', new Float32Array(24*1024*8), [1,24,1024,8]),
        cache_last_channel_len: new ort.Tensor('int64', new BigInt64Array([0n]), [1]),
        lang_id: new ort.Tensor('int64', BigInt64Array.from([33n]), [1])
    });
    const outLen = Number(r['encoded_lengths'].data[0]);
    console.log(`Encoder out: shape=${r['outputs'].dims}, len=${outLen}`);

    // Decode
    let decOut = (await dec.run({ targets: new ort.Tensor('int64', BigInt64Array.from([BigInt(config.blankId)]), [1,1]),
        h_in: new ort.Tensor('float32', new Float32Array(2*640), [2,1,640]), c_in: new ort.Tensor('float32', new Float32Array(2*640), [2,1,640]) }))['decoder_output'];
    console.log(`Decoder out shape: ${decOut.dims}`);

    const ed = r['outputs'].data;
    let tokens = 0;
    for (let t = 0; t < outLen; t++) {
        const fd = new Float32Array(1024); for (let d = 0; d < 1024; d++) fd[d] = ed[t*1024+d];
        const ef = new ort.Tensor('float32', fd, [1,1,1024]);
        const rd = new ort.Tensor('float32', decOut.data, [1,1,640]);
        const jr = await joi.run({ encoder_output: ef, decoder_output: rd });
        const lg = jr['joint_output'].data;
        const idx = Array.from(lg).map((v,i)=>({v,i})).sort((a,b)=>b.v-a.v);
        if (t === 0) { 
            console.log(`Frame 0 top-5: ${idx.slice(0,5).map(x=>`[${x.i}]=${x.v.toFixed(3)}`).join(' ')}`); 
            console.log(`  blank=${lg[config.blankId]?.toFixed(3)}`); 
        }
        if (idx[0].i !== config.blankId) { 
            tokens++; 
            console.log(`  t=${t}: tok=${idx[0].i}`); 
        }
    }
    console.log(`\nNon-blank tokens: ${tokens}/${outLen}`);
    await enc.release(); await dec.release(); await joi.release();
}
main().catch(console.error);
