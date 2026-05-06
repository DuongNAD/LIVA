const fs = require('fs');

function fixFile(filePath) {
    if (!fs.existsSync(filePath)) return;
    let content = fs.readFileSync(filePath, 'utf8');

    // 1. Fix "errMsg : String(e)" typo
    content = content.replace(/const errMsg = e instanceof Error \? errMsg : String\(e\);/g, 'const errMsg = e instanceof Error ? e.message : String(e);');
    
    // 2. Fix e.causerrMsg
    content = content.replace(/e\.causerrMsg/g, "e instanceof Error && 'cause' in e && e.cause instanceof Error ? e.cause.message : (e instanceof Error ? e.message : String(e))");
    content = content.replace(/const errMsg = e instanceof Error \? e instanceof Error && 'cause' in e && e\.cause instanceof Error \? e\.cause\.message : \(e instanceof Error \? e\.message : String\(e\)\) : String\(e\);/g, "const errMsg = e instanceof Error && 'cause' in e && e.cause instanceof Error ? e.cause.message : e instanceof Error ? e.message : String(e);");

    // 3. Redeclaration error fixing:
    // If multiple `const errMsg = ` exist in the same scope, we need to fix them. Let's see what happens if we change ALL `const errMsg = ` inside the file to unique names?
    // Actually, maybe I can just do a regex replace to match catch blocks.
    
    fs.writeFileSync(filePath, content, 'utf8');
}

fixFile('src/MemoryManager.ts');
fixFile('src/core/ModelOrchestrator.ts');
console.log('Fixed syntax errors');
