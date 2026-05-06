const fs = require('fs');

function cleanFile(filePath) {
    if (!fs.existsSync(filePath)) return;
    let content = fs.readFileSync(filePath, 'utf8');

    // Remove duplicates of `const errMsg = e instanceof Error ? e.message : String(e);`
    content = content.replace(/(const errMsg = e instanceof Error \? e\.message : String\(e\);\s*)+/g, 'const errMsg = e instanceof Error ? e.message : String(e);\n');

    // Fix causerrMsg typo and duplicate
    content = content.replace(/e\.causerrMsg/g, 'e.message');
    content = content.replace(/(const errMsg = e instanceof Error && 'cause' in e && e\.cause instanceof Error \? e\.cause\.message : e instanceof Error \? e\.message : String\(e\);\s*)+/g, 'const errMsg = e instanceof Error && \\'cause\\' in e && e.cause instanceof Error ? e.cause.message : e instanceof Error ? e.message : String(e);\n');

    fs.writeFileSync(filePath, content, 'utf8');
}

cleanFile('src/MemoryManager.ts');
cleanFile('src/core/ModelOrchestrator.ts');
console.log('Fixed duplicates');
