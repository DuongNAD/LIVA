const fs = require('fs');

const file = 'src/MemoryManager.ts';
let content = fs.readFileSync(file, 'utf8');

// Add EncryptionEngine import if missing
if (!content.includes('import { EncryptionEngine }')) {
    content = content.replace(
        'import type OpenAI from "openai";',
        'import { EncryptionEngine } from "./memory/EncryptionEngine";\nimport type OpenAI from "openai";'
    );
}

// Remove old ENCRYPTION_KEY and functions
const blockToRemove = /const ENCRYPTION_KEY = [\s\S]*?function decryptData[\s\S]*?return text;.*?\n  \}\n\}/;
content = content.replace(blockToRemove, '');

// Replace function calls
content = content.replace(/encryptData\(/g, 'EncryptionEngine.encrypt(');
content = content.replace(/decryptData\(/g, 'EncryptionEngine.decrypt(');

fs.writeFileSync(file, content);
console.log('Removed encryption functions and updated calls.');
