const fs = require('fs');
const path = require('path');

const inputFile = path.join(__dirname, 'repomix-liva-core.md');
const content = fs.readFileSync(inputFile, 'utf-8');
const lines = content.split('\n');

const numParts = 10;
const linesPerPart = Math.ceil(lines.length / numParts);

for (let i = 0; i < numParts; i++) {
    const start = i * linesPerPart;
    const end = Math.min(start + linesPerPart, lines.length);
    const chunk = lines.slice(start, end).join('\n');
    
    const outputFile = path.join(__dirname, `repomix-part-${i + 1}.md`);
    fs.writeFileSync(outputFile, chunk, 'utf-8');
    console.log(`Đã tạo: repomix-part-${i + 1}.md (${end - start} dòng)`);
}

console.log('Hoàn tất chia nhỏ thành 10 file!');
