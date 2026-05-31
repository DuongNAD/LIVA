import { env } from "@huggingface/transformers";
console.log("env.backends:", env.backends);
console.log("env.localModelPath:", env.localModelPath);
console.log("env.allowLocalModels:", env.allowLocalModels);
console.log("env.allowRemoteModels:", env.allowRemoteModels);
process.exit(0);
