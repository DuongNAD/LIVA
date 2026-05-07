const fs = require('fs');

// 1. AIScientist.ts
let file1 = 'src/skills/agentic/AIScientist.ts';
let code1 = fs.readFileSync(file1, 'utf-8');
code1 = code1.replace(/const errMsg = errMsg \|\| "";/g, '');
fs.writeFileSync(file1, code1);

// 2. ResearchIdeation.ts
let file2 = 'src/skills/agentic/ResearchIdeation.ts';
let code2 = fs.readFileSync(file2, 'utf-8');
code2 = code2.replace(/logger\.error\("Ideation Lỗi Parser:", err\);/g, 'logger.error({ err }, "Ideation Lỗi Parser:");');
code2 = code2.replace(/logger\.error\("Ideation Lỗi Parser:", errMsg\);/g, 'logger.error({ err: errMsg }, "Ideation Lỗi Parser:");');
code2 = code2.replace(/logger\.error\("Ideation Lỗi Parser:", e\);/g, 'logger.error({ err: e }, "Ideation Lỗi Parser:");');
fs.writeFileSync(file2, code2);

// 3. UpdateSessionState.ts
let file3 = 'src/skills/core/UpdateSessionState.ts';
let code3 = fs.readFileSync(file3, 'utf-8');
code3 = code3.replace(/from "\.\.\/types\/Contracts"/g, 'from "../../types/Contracts"');
fs.writeFileSync(file3, code3);

// 4. StructuredDataAnalyzer.ts
let file4 = 'src/skills/data/StructuredDataAnalyzer.ts';
let code4 = fs.readFileSync(file4, 'utf-8');
code4 = code4.replace(/if \(err\.code ===/g, 'if ((err as any).code ===');
fs.writeFileSync(file4, code4);

// 5. GitNexusQuery.ts
let file5 = 'src/skills/devops/GitNexusQuery.ts';
let code5 = fs.readFileSync(file5, 'utf-8');
code5 = code5.replace(/LanceMemoryManager\.getInstance\(\)/g, '(new LanceMemoryManager())');
fs.writeFileSync(file5, code5);

console.log("Fixed all remaining bugs!");
