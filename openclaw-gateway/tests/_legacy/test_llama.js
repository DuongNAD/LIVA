import { getLlama } from 'node-llama-cpp';

async function test() {
    try {
        console.log("Loading default node-llama-cpp Engine...");
        const llama = await getLlama();
        const model = await llama.loadModel({ 
            modelPath: "E:\\AI_Models\\Qwen2.5-7B-Instruct-Q8_0.gguf" 
        });
        console.log("Success! Qwen Model loaded via Native Node-Llama-Cpp!");
        model.dispose();
    } catch(e) {
        console.error("Failed:", e);
    }
}
test();
