self.onmessage = async (e) => {
  if (e.data.type === 'DECODE_AUDIO') {
    const { id, base64 } = e.data;
    try {
      // 1. Decode base64 string to Uint8Array
      const binaryStr = atob(base64);
      const len = binaryStr.length;
      const bytes = new Uint8Array(len);
      for (let i = 0; i < len; i++) {
        bytes[i] = binaryStr.codePointAt(i)!;
      }

      // 2. Decode MP3 to PCM using OfflineAudioContext (Web Worker Supported)
      const offlineCtx = new OfflineAudioContext(2, 1, 44100);
      const audioBuffer = await offlineCtx.decodeAudioData(bytes.buffer);

      const channels = audioBuffer.numberOfChannels;
      const length = audioBuffer.length;
      const sampleRate = audioBuffer.sampleRate;

      // Extract PCM Float32Array for each channel and copy to transferable buffers
      const pcmChannels: Float32Array[] = [];
      const transferList: ArrayBuffer[] = [];
      
      for (let c = 0; c < channels; c++) {
        const floatArr = audioBuffer.getChannelData(c);
        const copyArr = new Float32Array(floatArr);
        pcmChannels.push(copyArr);
        transferList.push(copyArr.buffer);
      }

      // 3. Pre-calculate Lip-Sync envelope (RMS per frame for 60fps)
      const fps = 60;
      const frameSize = Math.floor(sampleRate / fps);
      const framesCount = Math.floor(length / frameSize);
      const lipSyncData = new Float32Array(framesCount);
      
      const rawData = pcmChannels[0];
      for (let i = 0; i < framesCount; i++) {
        let sum = 0;
        const start = i * frameSize;
        for (let j = 0; j < frameSize; j++) {
          const val = rawData[start + j];
          sum += val * val;
        }
        // RMS (Root Mean Square) scaled to 0-255 amplitude range for easy consumption
        lipSyncData[i] = Math.min(255, Math.floor(Math.sqrt(sum / frameSize) * 1000));
      }
      transferList.push(lipSyncData.buffer);

      // 4. Send back Float32Array to Main Thread
      self.postMessage({
        type: 'AUDIO_READY',
        id,
        channels,
        length,
        sampleRate,
        pcmChannels,
        lipSyncData
      }, { transfer: transferList });
      
    } catch (err: any) {
      self.postMessage({ type: 'AUDIO_ERROR', id, error: err.message });
    }
  }
};
