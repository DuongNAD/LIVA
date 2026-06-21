const fs = require('fs');
const content = fs.readFileSync('tests/core/AgentLoopBargeIn.test.ts', 'utf8');

const regex1 = /vi\.mock\((['"])openai\1,\s*\(\s*\)\s*=>\s*\(\{\s*default:/g;
const regex2 = /vi\.mock\((['"])openai\1,\s*\(\s*\)\s*=>\s*\{\s*([\s\S]*?)return\s*\{\s*default:/g;

console.log('Regex1 matches:', regex1.test(content));
console.log('Regex2 matches:', regex2.test(content));

const modified = content.replace(regex2, 'vi.mock($1openai$1, () => { $2 return { __esModule: true, default:');
console.log('Modified section:');
const start = modified.indexOf('vi.mock("openai"');
if (start !== -1) {
    console.log(modified.substring(start, start + 300));
} else {
    console.log('vi.mock("openai" not found in modified');
}
