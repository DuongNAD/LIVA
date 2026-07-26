const DEFAULT_FRAME_SIZE = 512;

class LivaMicCaptureProcessor extends AudioWorkletProcessor {
  constructor(options) {
    super();
    this.frameSize = options.processorOptions?.frameSize ?? DEFAULT_FRAME_SIZE;
    this.frame = new Float32Array(this.frameSize);
    this.offset = 0;
  }

  process(inputs, outputs) {
    const input = inputs[0]?.[0];
    const output = outputs[0]?.[0];

    // Keep the node connected so WebView continues invoking process(), but never
    // route microphone samples to the speakers.
    output?.fill(0);

    if (!input) {
      return true;
    }

    let inputOffset = 0;
    while (inputOffset < input.length) {
      const available = this.frameSize - this.offset;
      const count = Math.min(available, input.length - inputOffset);
      this.frame.set(input.subarray(inputOffset, inputOffset + count), this.offset);
      this.offset += count;
      inputOffset += count;

      if (this.offset === this.frameSize) {
        this.port.postMessage(this.frame, [this.frame.buffer]);
        this.frame = new Float32Array(this.frameSize);
        this.offset = 0;
      }
    }

    return true;
  }
}

registerProcessor("liva-mic-capture", LivaMicCaptureProcessor);
