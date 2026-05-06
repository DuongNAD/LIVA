const fs = require('fs');

function fixCatch(file) {
    let content = fs.readFileSync(file, 'utf8');
    
    // Replace catch (e: any) with unknown + errMsg
    let regex = /catch\s*\(\s*(e|err)\s*:\s*any\s*\)\s*\{/g;
    content = content.replace(regex, 'catch ($1: unknown) {\n            const errMsg = $1 instanceof Error ? $1.message : String($1);');
    
    // Also fix cases where it's already unknown but missing errMsg
    let regexUnk = /catch\s*\(\s*(e|err)\s*:\s*unknown\s*\)\s*\{(\s*)(?!(const|let|var)\s+errMsg)/g;
    content = content.replace(regexUnk, 'catch ($1: unknown) {$2const errMsg = $1 instanceof Error ? $1.message : String($1);\n$2');
    
    // Replace e.message and err.message with errMsg
    content = content.replace(/e\.message/g, 'errMsg');
    content = content.replace(/err\.message/g, 'errMsg');
    
    fs.writeFileSync(file, content);
    console.log('Fixed catches in ' + file);
}

fixCatch('src/core/ModelOrchestrator.ts');
fixCatch('src/MemoryManager.ts');
