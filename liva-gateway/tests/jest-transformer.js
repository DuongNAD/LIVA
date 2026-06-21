import { createRequire } from 'node:module';
const require = createRequire(import.meta.url);
const tsJest = require('ts-jest').default;

const tsJestTransformer = tsJest.createTransformer({
  tsconfig: {
    module: 'CommonJS',
    moduleResolution: 'node',
    esModuleInterop: true,
    skipLibCheck: true,
  },
  isolatedModules: true,
});

export default {
  process(sourceText, sourcePath, options) {
    let modifiedText = sourceText;

    // Comment out createRequire import and require redeclaration
    modifiedText = modifiedText.replace(/import\s*\{\s*createRequire\s*\}\s*from\s*(['"])node:module\1\s*;?/g, '// import { createRequire } from \'node:module\';');
    modifiedText = modifiedText.replace(/const\s+require\s*=\s*createRequire\([^)]*\)\s*;?/g, '// const require = ...');

    // 1. Replace import.meta with CommonJS equivalents
    // import.meta.dirname ?? path.dirname(fileURLToPath(import.meta.url)) -> __dirname
    modifiedText = modifiedText.replace(/import\.meta\.dirname\s*\?\?\s*path\.dirname\(fileURLToPath\(import\.meta\.url\)\)/g, '__dirname');
    // import.meta.dirname -> __dirname
    modifiedText = modifiedText.replace(/import\.meta\.dirname/g, '__dirname');
    // import.meta.filename -> __filename
    modifiedText = modifiedText.replace(/import\.meta\.filename/g, '__filename');
    // import.meta.url -> (new URL('file://' + __filename).href)
    modifiedText = modifiedText.replace(/import\.meta\.url/g, "new URL('file://' + __filename).href");

    if (sourcePath.endsWith('.test.ts') || sourcePath.includes('tests/')) {
      if (!sourcePath.endsWith('vitest-compat-bridge.ts') && !sourcePath.endsWith('sqlite-vec-compat.ts') && !sourcePath.endsWith('jest-transformer.js')) {
        modifiedText = "import { jest } from '@jest/globals';\n" + modifiedText;
      }
      // Replace await import(...) with require(...) inside tests to support synchronous mock factories
      modifiedText = modifiedText.replace(/\bawait\s+import\s*\((['"`])(.+?)\1\)/g, 'require($1$2$1)');

      // 1.5 Replace await importOriginal with importOriginal
      modifiedText = modifiedText.replace(/\bawait\s+importOriginal\b/g, 'importOriginal');

      // 1.6 Replace await vi.importActual with jest.requireActual
      modifiedText = modifiedText.replace(
        /\bawait\s+vi\.importActual\((['"`])(.+?)\1\)/g,
        (match, quote, moduleName) => `jest.requireActual(${quote}${moduleName}${quote})`
      );

      // 1.9 Auto-inject __esModule: true in vi.mock("openai", ...)
      modifiedText = modifiedText.replace(/vi\.mock\((['"])openai\1,\s*\(\s*\)\s*=>\s*\(\{\s*default:/g, 'vi.mock($1openai$1, () => ({ __esModule: true, default:');
      modifiedText = modifiedText.replace(/vi\.mock\((['"])openai\1,\s*\(\s*\)\s*=>\s*\{\s*([\s\S]*?)return\s*\{\s*default:/g, 'vi.mock($1openai$1, () => { $2 return { __esModule: true, default:');

      // 2. Replace vi.mock with importOriginal parameter mapping
      modifiedText = modifiedText.replace(
        /\bvi\.mock\((['"`])(.+?)\1,\s*async\s*\((.*?)\)\s*=>\s*\{/g,
        (match, quote, moduleName) => {
          return `jest.mock(${quote}${moduleName}${quote}, () => { const importOriginal = () => jest.requireActual(${quote}${moduleName}${quote});`;
        }
      );
      modifiedText = modifiedText.replace(
        /\bvi\.mock\((['"`])(.+?)\1,\s*\(\s*importOriginal\s*\)\s*=>\s*\{/g,
        (match, quote, moduleName) => {
          return `jest.mock(${quote}${moduleName}${quote}, () => { const importOriginal = () => jest.requireActual(${quote}${moduleName}${quote});`;
        }
      );
      
      // 3. Replace other vi.mock/unmock/doMock/doUnmock calls
      modifiedText = modifiedText
        .replace(/\bvi\.mock\(/g, 'jest.mock(')
        .replace(/\bvi\.unmock\(/g, 'jest.unmock(')
        .replace(/\bvi\.doMock\(/g, 'jest.doMock(')
        .replace(/\bvi\.doUnmock\(/g, 'jest.unmock(');
    }

    return tsJestTransformer.process(modifiedText, sourcePath, options);
  },
  getCacheKey(sourceText, sourcePath, options) {
    return tsJestTransformer.getCacheKey(sourceText, sourcePath, options);
  }
};
