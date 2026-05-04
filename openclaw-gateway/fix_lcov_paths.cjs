const fs = require('fs');
const path = require('path');

const lcovPath = path.join(__dirname, 'coverage', 'lcov.info');
if (!fs.existsSync(lcovPath)) {
  console.log('lcov.info not found, skipping path fix.');
  process.exit(0);
}

const absoluteSrcPath = path.join(__dirname, 'src');

let content = fs.readFileSync(lcovPath, 'utf8');

// The relative paths from vitest usually start with 'src/' or 'src\'
// We replace 'SF:src/' or 'SF:src\' with 'SF:E:\project\openclaw_remake\openclaw-gateway\src\'
// To be safe and cross-platform, we resolve the absolute path and ensure backslashes.

content = content.replace(/^SF:(.+)$/gm, (match, p1) => {
  if (path.isAbsolute(p1)) {
     return `SF:${p1.replace(/\//g, '\\')}`;
  }
  
  if (p1.startsWith('src/') || p1.startsWith('src\\')) {
     let newPath = path.join(__dirname, p1);
     newPath = newPath.replace(/\//g, '\\');
     return `SF:${newPath}`;
  }
  
  return match;
});

fs.writeFileSync(lcovPath, content, 'utf8');
console.log('Successfully rewrote LCOV file paths to absolute backslashes for SonarQube.');
