import fs from 'node:fs';
import path from 'node:path';

const file = path.resolve('src/MemoryManager.ts');
const query = 'getPreviousSessionContextPrompt';

console.log(`Searching for "${query}" in ${file}:`);
const lines = fs.readFileSync(file, 'utf8').split('\n');
lines.forEach((line, idx) => {
    if (line.includes(query)) {
        console.log(`${idx + 1}: ${line.trim()}`);
    }
});
