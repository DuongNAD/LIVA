import { execSync } from 'child_process';
import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';

// Resolve __dirname under ESM
const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

// Banned packages as defined in the linter and AI_CONTEXT guidelines
const BANNED_PACKAGES = [
  'axios',
  'got',
  'sqlite3',
  'node-llama-cpp',
  'transformers',
  '@xenova/transformers',
  '@huggingface/transformers',
  '@lancedb/lancedb',
  'puppeteer',
  'request',
  'node-fetch',
  'fuse.js',
  'sqlite'
];

interface FileProfile {
  path: string;
  sizeBytes: number;
  lineCount: number;
  isGodComponent: boolean;
}

interface BannedImportOccurrence {
  file: string;
  line: number;
  imported: string;
}

interface BannedDepOccurrence {
  packageJson: string;
  depType: string;
  name: string;
  version: string;
}

async function runAudit() {
  console.log('Starting LIVA Codebase Audit Scanner...');
  const rootDir = path.resolve(__dirname, '..');

  // 1. Run TypeScript Type Check
  console.log('Running TypeScript compiler check...');
  let tsErrorsCount = 0;
  let tscOutput = '';
  try {
    // Run tsc on the liva-gateway workspace (using tests/tsconfig.json to cover both src and tests)
    tscOutput = execSync('npx tsc --noEmit -p liva-gateway/tests/tsconfig.json', {
      cwd: rootDir,
      encoding: 'utf8',
      maxBuffer: 20 * 1024 * 1024
    });
  } catch (error: any) {
    tscOutput = error.stdout || error.stderr || '';
  }
  
  const tsErrorLines = tscOutput.split('\n').filter(line => /\.tsx?\(.*?\):\s+error\s+TS\d+:/.test(line));
  tsErrorsCount = tsErrorLines.length;
  console.log(`TypeScript check completed with ${tsErrorsCount} errors.`);

  // 2. Run ESLint check
  console.log('Running ESLint analysis...');
  let eslintErrorsCount = 0;
  let eslintWarningsCount = 0;
  let eslintOutputJson = '[]';
  try {
    eslintOutputJson = execSync('npx eslint --format json liva-gateway/src', {
      cwd: rootDir,
      encoding: 'utf8',
      maxBuffer: 20 * 1024 * 1024
    });
  } catch (error: any) {
    // ESLint exits with code 1 if errors are found; stdout still contains JSON
    eslintOutputJson = error.stdout || '[]';
  }

  let eslintResults: any[] = [];
  try {
    eslintResults = JSON.parse(eslintOutputJson);
  } catch (e) {
    console.error('Failed to parse ESLint JSON output. Using fallback empty array.');
  }

  for (const result of eslintResults) {
    eslintErrorsCount += result.errorCount || 0;
    eslintWarningsCount += result.warningCount || 0;
  }
  console.log(`ESLint check completed with ${eslintErrorsCount} errors and ${eslintWarningsCount} warnings.`);

  // 3. Scan Files for God Components and Banned Imports
  console.log('Scanning source files...');
  const allFiles: FileProfile[] = [];
  const bannedImports: BannedImportOccurrence[] = [];
  
  function scanDirectory(dir: string) {
    const list = fs.readdirSync(dir);
    for (const file of list) {
      const fullPath = path.join(dir, file);
      const stat = fs.statSync(fullPath);
      if (stat && stat.isDirectory()) {
        const relativeDirName = path.basename(fullPath);
        if (['node_modules', 'dist', 'coverage', '.agents', '.git', '.liva_workspaces', '.liva_shield'].includes(relativeDirName)) {
          continue;
        }
        scanDirectory(fullPath);
      } else if (file.endsWith('.ts') && !file.endsWith('.d.ts')) {
        const relativePath = path.relative(rootDir, fullPath);
        const content = fs.readFileSync(fullPath, 'utf8');
        const lines = content.split(/\r?\n/);
        const lineCount = lines.length;
        const sizeBytes = stat.size;
        const isGodComponent = lineCount > 1200;

        allFiles.push({
          path: relativePath,
          sizeBytes,
          lineCount,
          isGodComponent
        });

        // Scan for banned package imports
        lines.forEach((line, idx) => {
          // Match import or require statements
          const importRequireRegex = /(?:import|require)\s*\(?\s*['"]([^'"]+)['"]/g;
          let match;
          while ((match = importRequireRegex.exec(line)) !== null) {
            const importedPkg = match[1];
            const parts = importedPkg.split('/');
            const basePkg = importedPkg.startsWith('@') ? parts.slice(0, 2).join('/') : parts[0];
            if (BANNED_PACKAGES.includes(basePkg)) {
              bannedImports.push({
                file: relativePath,
                line: idx + 1,
                imported: importedPkg
              });
            }
          }
        });
      }
    }
  }

  // Scan liva-gateway/src, packages/liva-common/src, liva-desktop/src, liva-ui/src
  const srcDirs = [
    path.join(rootDir, 'liva-gateway', 'src'),
    path.join(rootDir, 'packages', 'liva-common', 'src'),
    path.join(rootDir, 'liva-desktop', 'src'),
    path.join(rootDir, 'liva-ui', 'src')
  ];

  for (const srcDir of srcDirs) {
    if (fs.existsSync(srcDir)) {
      scanDirectory(srcDir);
    }
  }

  const godComponents = allFiles.filter(f => f.isGodComponent);
  console.log(`Scanned ${allFiles.length} TS files. Found ${godComponents.length} God components.`);

  // 4. Scan package.json Files for Banned Packages and Dependency Counts
  console.log('Scanning package.json files for dependencies...');
  const packageJsonFiles = [
    'package.json',
    'packages/liva-common/package.json',
    'liva-gateway/package.json',
    'liva-ui/package.json',
    'liva-desktop/package.json'
  ];

  const bannedDeps: BannedDepOccurrence[] = [];
  const dependencyCounts: Record<string, { dependencies: number; devDependencies: number; total: number }> = {};
  let totalDependenciesCount = 0;

  for (const pkgFile of packageJsonFiles) {
    const fullPath = path.join(rootDir, pkgFile);
    if (fs.existsSync(fullPath)) {
      const content = JSON.parse(fs.readFileSync(fullPath, 'utf8'));
      const depTypes = ['dependencies', 'devDependencies', 'peerDependencies', 'optionalDependencies'];
      
      let depsCount = 0;
      let devDepsCount = 0;

      for (const depType of depTypes) {
        if (content[depType]) {
          const keys = Object.keys(content[depType]);
          if (depType === 'dependencies' || depType === 'optionalDependencies') depsCount += keys.length;
          if (depType === 'devDependencies') devDepsCount += keys.length;

          for (const [name, version] of Object.entries(content[depType] as Record<string, string>)) {
            totalDependenciesCount++;
            if (BANNED_PACKAGES.includes(name)) {
              bannedDeps.push({
                packageJson: pkgFile,
                depType,
                name,
                version
              });
            }
          }
        }
      }

      dependencyCounts[pkgFile] = {
        dependencies: depsCount,
        devDependencies: devDepsCount,
        total: depsCount + devDepsCount
      };
    }
  }

  console.log(`Scan complete. Found ${bannedDeps.length} banned dependencies in package.json files.`);

  // 5. Compute Architecture Health Score
  // Score Formula: 100 - (5 * godComponentsCount) - (5 * tsErrorsCount) - (2 * violationsCount)
  // violationsCount = eslintErrors + eslintWarnings + bannedImportsCount + bannedDepsCount
  const godComponentsCount = godComponents.length;
  const violationsCount = eslintErrorsCount + eslintWarningsCount + bannedImports.length + bannedDeps.length;
  
  let score = 100 - (5 * godComponentsCount) - (5 * tsErrorsCount) - (2 * violationsCount);
  score = Math.max(0, score);
  const codeRedTriggered = score < 70;

  console.log(`Final calculated Architecture Health Score: ${score}`);
  console.log(`Code Red status: ${codeRedTriggered ? 'TRIGGERED (CODE RED)' : 'NORMAL'}`);

  // 6. Save Scan Results JSON
  const resultsDir = path.join(rootDir, 'logs');
  if (!fs.existsSync(resultsDir)) {
    fs.mkdirSync(resultsDir, { recursive: true });
  }

  const scanResults = {
    timestamp: new Date().toISOString(),
    score,
    codeRedTriggered,
    metrics: {
      tsErrorsCount,
      eslintErrorsCount,
      eslintWarningsCount,
      godComponentsCount,
      bannedImportsCount: bannedImports.length,
      bannedDepsCount: bannedDeps.length,
      totalDependencies: totalDependenciesCount,
      totalTsFiles: allFiles.length
    },
    godComponents: godComponents.map(f => ({
      path: f.path,
      lineCount: f.lineCount,
      sizeBytes: f.sizeBytes
    })),
    bannedImports,
    bannedDeps,
    dependencyCounts,
    fileSizes: allFiles.map(f => ({
      path: f.path,
      sizeBytes: f.sizeBytes,
      lineCount: f.lineCount
    }))
  };

  const resultsPath = path.join(resultsDir, 'audit_scan_results.json');
  fs.writeFileSync(resultsPath, JSON.stringify(scanResults, null, 2), 'utf8');
  console.log(`Saved detailed scan results to ${resultsPath}`);

  // 7. Update tech-debt-ledger.json
  const ledgerPath = path.join(rootDir, 'tech-debt-ledger.json');
  let ledgerData = { ledger: [] as any[] };
  if (fs.existsSync(ledgerPath)) {
    try {
      ledgerData = JSON.parse(fs.readFileSync(ledgerPath, 'utf8'));
    } catch (e) {
      console.error('Failed to parse tech-debt-ledger.json. Initializing a new ledger.');
    }
  }

  ledgerData.ledger.push({
    timestamp: new Date().toISOString(),
    score,
    godComponentsCount,
    violationsCount,
    codeRedTriggered
  });

  fs.writeFileSync(ledgerPath, JSON.stringify(ledgerData, null, 2), 'utf8');
  console.log(`Updated technical debt ledger at ${ledgerPath}`);
}

runAudit().catch(err => {
  console.error('Audit run failed with error:', err);
  process.exit(1);
});
