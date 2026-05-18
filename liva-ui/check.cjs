const fs = require('fs');
const txt = fs.readFileSync('e:/Project/LIVA/liva-ui/src/WidgetApp.vue', 'utf8').split('</script>')[0];
let braces = 0;
let parens = 0;
let inString = false;
let stringChar = '';
const lines = txt.split('\n');

let lastB = 0;
for (let i = 0; i < lines.length; i++) {
  const l = lines[i];
  for (let j = 0; j < l.length; j++) {
    const c = l[j];
    if (inString) {
      if (c === '\\') { j++; continue; }
      if (c === stringChar) { inString = false; }
      continue;
    }
    if (c === '"' || c === "'" || c === '`') {
      inString = true;
      stringChar = c;
      continue;
    }
    if (c === '/' && l[j+1] === '/') break;
    
    if (c === '{') braces++;
    if (c === '}') braces--;
    if (c === '(') parens++;
    if (c === ')') parens--;
  }
  if (braces !== lastB) {
    console.log(`L${i+1}: ${braces}`);
    lastB = braces;
  }
}
console.log(`Final: braces=${braces} parens=${parens}`);
