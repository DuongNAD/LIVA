const fs = require('fs');
let file = 'src/memory/StructuredMemory.ts';
let code = fs.readFileSync(file, 'utf-8');

code = code.replace(/const errMsg = e instanceof Error \? errMsg : String\(e\);/g, 'const errMsg = e instanceof Error ? e.message : String(e);');
code = code.replace(/const ttlMs = row\.ttlDays \* 24 \* 60 \* 60 \* 1000;/g, 'if (row.ttlDays === null) continue;\n            const ttlMs = row.ttlDays * 24 * 60 * 60 * 1000;');

fs.writeFileSync(file, code);
console.log("Fixes applied!");
