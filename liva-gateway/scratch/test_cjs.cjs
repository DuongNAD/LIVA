const { pipeline, env } = require("@huggingface/transformers");
console.log("CJS Import success! pipeline:", typeof pipeline);
console.log("env.localModelPath:", env.localModelPath);
process.exit(0);
