const fs = require('fs');

let content = fs.readFileSync('src/memory/HeraCompass.ts', 'utf8');

content = content.replace(
    /export interface HeraInsight \{/,
    `export interface HeraInsight {
    [key: string]: any;`
);

content = content.replace(
    /const results = this\.flexIndex\.search\(failedContext, 5\) as any\[\];/,
    `const results = (this.flexIndex.search(failedContext, 5) || []) as any[];`
);

fs.writeFileSync('src/memory/HeraCompass.ts', content);
console.log('Fixed HeraCompass.ts TS errors');
