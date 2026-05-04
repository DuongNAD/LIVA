const { execSync } = require('child_process');
const fs = require('fs');
const path = require('path');

const TESTS_DIR = path.join(__dirname, '../tests');
const COVERAGE_DIR = path.join(__dirname, '../coverage');
const RAW_DIR = path.join(COVERAGE_DIR, 'raw');

if (fs.existsSync(COVERAGE_DIR)) {
  fs.rmSync(COVERAGE_DIR, { recursive: true, force: true });
}
fs.mkdirSync(RAW_DIR, { recursive: true });

const dirs = fs.readdirSync(TESTS_DIR, { withFileTypes: true })
  .filter(dirent => dirent.isDirectory() && !dirent.name.startsWith('_'))
  .map(dirent => dirent.name);

console.log(`[CI Coverage] Found test directories: ${dirs.join(', ')}`);

let hasError = false;

for (const dir of dirs) {
  console.log(`\n[CI Coverage] Running coverage for tests/${dir}...`);
  try {
    // Run vitest for the specific directory with json reporter
    execSync(`npx vitest run tests/${dir}/ --coverage --coverage.reporter=json --coverage.reportsDirectory=coverage/raw/${dir} --coverage.thresholds.lines=0 --coverage.thresholds.statements=0 --coverage.thresholds.branches=0 --coverage.thresholds.functions=0`, {
      stdio: 'inherit',
      env: { ...process.env, VITEST_MIN_THREADS: '1' }
    });
  } catch (err) {
    console.error(`[CI Coverage] Error running tests in tests/${dir}`);
    hasError = true;
  }
}

console.log('\n[CI Coverage] Merging coverage reports...');
try {
  // Use nyc merge to combine json files
  const jsonFiles = fs.readdirSync(RAW_DIR)
    .map(d => path.join(RAW_DIR, d, 'coverage-final.json'))
    .filter(f => fs.existsSync(f));

  if (jsonFiles.length === 0) {
    throw new Error("No coverage-final.json files found!");
  }

  // Copy files to a flat directory for nyc merge
  const FLAT_RAW_DIR = path.join(COVERAGE_DIR, 'flat_raw');
  fs.mkdirSync(FLAT_RAW_DIR, { recursive: true });
  
  jsonFiles.forEach((file, index) => {
    fs.copyFileSync(file, path.join(FLAT_RAW_DIR, `coverage-${index}.json`));
  });

  // Create a combined JSON by merging them
  execSync(`npx nyc merge coverage/flat_raw coverage/coverage.json`, { stdio: 'inherit' });

  // Generate lcov and text reports
  console.log('\n[CI Coverage] Generating merged reports...');
  execSync(`npx nyc report -t coverage --report-dir coverage --reporter=lcov --reporter=text --reporter=text-summary`, { stdio: 'inherit' });

  // Fix paths in lcov.info for SonarQube (prepend openclaw-gateway/ to SF:src/ paths)
  const lcovPath = path.join(COVERAGE_DIR, 'lcov.info');
  if (fs.existsSync(lcovPath)) {
    let lcovContent = fs.readFileSync(lcovPath, 'utf8');
    // Handle both Windows (\) and Unix (/) paths
    lcovContent = lcovContent.replace(/^SF:src[\\/]/gm, 'SF:openclaw-gateway/src/');
    // Normalize ALL remaining backslashes in SF: lines to forward slashes
    // SonarQube cannot match mixed-separator paths like src/memory\HeraCompass.ts
    lcovContent = lcovContent.replace(/^(SF:.*)$/gm, (match) => match.replace(/\\/g, '/'));
    fs.writeFileSync(lcovPath, lcovContent, 'utf8');
    console.log('[CI Coverage] Fixed paths in lcov.info for SonarQube compatibility.');
  }

  if (hasError) {
    console.error('\n[CI Coverage] Some test suites failed, but coverage was merged.');
    process.exit(1);
  } else {
    console.log('\n[CI Coverage] Coverage generated successfully.');
  }

} catch (err) {
  console.error('\n[CI Coverage] Failed to merge coverage reports:', err);
  process.exit(1);
}
