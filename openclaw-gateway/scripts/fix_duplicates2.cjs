const fs = require('fs');

function cleanFile(filePath) {
    if (!fs.existsSync(filePath)) return;
    let content = fs.readFileSync(filePath, 'utf8');

    // Remove duplicates of `const errMsg = e instanceof Error ? e.message : String(e);`
    content = content.replace(/(const errMsg = e instanceof Error \? e\.message : String\(e\);\s*)+/g, 'const errMsg = e instanceof Error ? e.message : String(e);\n');

    // Fix causerrMsg typo
    content = content.replace(/e\.causerrMsg/g, 'e.message');
    // Also fix the crazy long nested string if it exists
    content = content.replace(/(const errMsg = e instanceof Error && 'cause' in e && e\.cause instanceof Error \? e\.cause\.message : \(e instanceof Error \? e\.message : String\(e\)\);\s*)+/g, `const errMsg = e instanceof Error && 'cause' in e && e.cause instanceof Error ? e.cause.message : e instanceof Error ? e.message : String(e);\n`);
    content = content.replace(/(const errMsg = e instanceof Error && 'cause' in e && e\.cause instanceof Error \? e instanceof Error && 'cause' in e && e\.cause instanceof Error \? e\.cause\.message : \(e instanceof Error \? e\.message : String\(e\)\) : e instanceof Error \? errMsg : String\(e\);\s*)+/g, `const errMsg = e instanceof Error && 'cause' in e && e.cause instanceof Error ? e.cause.message : e instanceof Error ? e.message : String(e);\n`);
    // Fallback: If any catch block has "const errMsg = " repeated in some weird way, let's just do a brutal replace:
    content = content.replace(/catch\s*\(\s*e\s*:\s*unknown\s*\)\s*\{\s*(const errMsg = e[^;]+;\s*)+/g, `catch (e: unknown) {\n          const errMsg = e instanceof Error && 'cause' in e && e.cause instanceof Error ? e.cause.message : e instanceof Error ? e.message : String(e);\n`);

    fs.writeFileSync(filePath, content, 'utf8');
}

cleanFile('src/MemoryManager.ts');
cleanFile('src/core/ModelOrchestrator.ts');
console.log('Fixed duplicates');
