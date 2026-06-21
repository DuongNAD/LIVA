const fs = require('fs');
const transformer = require('./tests/jest-transformer.js').default;

const content = fs.readFileSync('tests/core/AgentLoop.test.ts', 'utf8');
const result = transformer.process(content, 'tests/core/AgentLoop.test.ts', {
    config: {
        cwd: __dirname,
    }
});

const idx = result.code.indexOf('jest.mock("openai"');
if (idx !== -1) {
    console.log(result.code.substring(idx - 100, idx + 400));
} else {
    console.log('Not found');
    const idx2 = result.code.indexOf("jest.mock('openai'");
    if (idx2 !== -1) {
        console.log(result.code.substring(idx2 - 100, idx2 + 400));
    }
}
