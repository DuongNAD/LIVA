import { describe, it, expect } from 'vitest';
import * as fs from 'fs';
import * as path from 'path';
// @ts-ignore
import yaml from 'js-yaml';
import { validateVault } from '../scripts/validate-vault.js';

const vaultRoot = path.resolve(__dirname, '../vault');
const notePath = path.join(vaultRoot, 'Knowledge', 'deepseek_harness_integration.md');
const noteContent = fs.readFileSync(notePath, 'utf8');

describe('Empirical Challenger: DeepSeek Harness Integration Vault Note Verification', () => {
  
  describe('1. Vault Script Compliance & Structure', () => {
    it('passes automated validateVault check with 0 errors and 0 broken links', () => {
      const report = validateVault(vaultRoot);
      expect(report.invalidFilesCount).toBe(0);
      expect(report.brokenLinksCount).toBe(0);
      expect(report.errors).toHaveLength(0);

      const noteResult = report.fileResults.find(r => r.relativeDocPath.replace(/\\/g, '/') === 'Knowledge/deepseek_harness_integration.md');
      expect(noteResult).toBeDefined();
      expect(noteResult?.isValid).toBe(true);
      expect(noteResult?.errors).toHaveLength(0);
    });
  });

  describe('2. Frontmatter Isolation & Strict Schema Compliance', () => {
    const frontmatterRegex = /^---\r?\n([\s\S]*?)\r?\n---(?:\r?\n|$)/;
    const match = noteContent.match(frontmatterRegex);

    it('has well-isolated frontmatter block delimited by triple dashes', () => {
      expect(match).not.toBeNull();
    });

    it('parses frontmatter cleanly as YAML without syntax errors', () => {
      expect(match).not.toBeNull();
      const fm = yaml.load(match![1]) as Record<string, any>;
      expect(typeof fm).toBe('object');
      expect(fm).not.toBeNull();

      expect(fm.title).toBe('deepseek_harness_integration');
      expect(Array.isArray(fm.tags)).toBe(true);
      expect(fm.tags).toContain('liva/knowledge');
      expect(fm.tags).toContain('liva/architecture');
      expect(fm.tags).toContain('liva/deepseek');
      expect(fm.tags).toContain('liva/cordis');

      expect(fm.author).toBe('LIVA Core Architecture Team');
      expect(fm.confidence).toBe('high');
      expect(Array.isArray(fm.sources)).toBe(true);
      expect(fm.sources.length).toBeGreaterThan(0);

      // ISO 8601 validation
      const iso8601Regex = /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:\d{2})$/;
      expect(iso8601Regex.test(fm.last_update)).toBe(true);
      expect(isNaN(Date.parse(fm.last_update))).toBe(false);
    });
  });

  describe('3. Markdown Syntax & Code Blocks Verification', () => {
    it('maintains proper heading hierarchy without skipping levels', () => {
      const headingRegex = /^(#{1,6})\s+(.+)$/gm;
      let match;
      let prevLevel = 0;
      const headings: { level: number; text: string }[] = [];

      while ((match = headingRegex.exec(noteContent)) !== null) {
        const level = match[1].length;
        const text = match[2];
        headings.push({ level, text });
        
        // Cannot jump by more than 1 level down (e.g., H1 -> H3 is invalid)
        if (prevLevel > 0) {
          expect(level).toBeLessThanOrEqual(prevLevel + 1);
        }
        prevLevel = level;
      }
      expect(headings.length).toBeGreaterThan(5);
    });

    it('contains valid JSON code blocks', () => {
      const jsonBlockRegex = /```json\r?\n([\s\S]*?)\r?\n```/g;
      let match;
      let count = 0;
      while ((match = jsonBlockRegex.exec(noteContent)) !== null) {
        count++;
        const jsonContent = match[1];
        expect(() => JSON.parse(jsonContent)).not.toThrow();
        const parsed = JSON.parse(jsonContent);
        if (parsed.benchmark_suite) {
          expect(parsed.benchmark_suite).toBe('liva_core_v1');
          expect(Array.isArray(parsed.test_cases)).toBe(true);
          expect(parsed.test_cases.length).toBeGreaterThanOrEqual(3);
        }
      }
      expect(count).toBeGreaterThan(0);
    });

    it('contains syntactically well-formed Rust RFC specifications', () => {
      const rustBlockRegex = /```rust\r?\n([\s\S]*?)\r?\n```/g;
      let match;
      let count = 0;
      while ((match = rustBlockRegex.exec(noteContent)) !== null) {
        count++;
        const rustCode = match[1];
        
        // Verify bracket balance
        let braceCount = 0;
        let parenCount = 0;
        let bracketCount = 0;
        for (const char of rustCode) {
          if (char === '{') braceCount++;
          if (char === '}') braceCount--;
          if (char === '(') parenCount++;
          if (char === ')') parenCount--;
          if (char === '[') bracketCount++;
          if (char === ']') bracketCount--;
          expect(braceCount).toBeGreaterThanOrEqual(0);
          expect(parenCount).toBeGreaterThanOrEqual(0);
          expect(bracketCount).toBeGreaterThanOrEqual(0);
        }
        expect(braceCount).toBe(0);
        expect(parenCount).toBe(0);
        expect(bracketCount).toBe(0);

        // Verify key Rust definitions
        expect(rustCode).toMatch(/(pub\s+(struct|enum|trait|fn)|impl)/);
      }
      expect(count).toBeGreaterThanOrEqual(3);
    });

    it('has balanced LaTeX/Math expressions', () => {
      // Check $$ display math blocks
      const doubleDollarMatches = noteContent.match(/\$\$/g) || [];
      expect(doubleDollarMatches.length % 2).toBe(0);

      // Check single $ inline math pairs (ignoring escaped ones)
      const lines = noteContent.split('\n');
      for (const line of lines) {
        if (line.includes('$$')) continue; // Skip display math lines
        const singleDollarMatches = (line.match(/(?<!\\)\$/g) || []).length;
        expect(singleDollarMatches % 2).toBe(0);
      }
    });

    it('has properly formatted markdown tables with consistent column counts', () => {
      const tableLines = noteContent.split(/\r?\n/).filter(line => line.trim().startsWith('|') && line.trim().endsWith('|'));
      expect(tableLines.length).toBeGreaterThan(2);

      const parseCols = (line: string) => line.trim().slice(1, -1).split('|').map(s => s.trim());

      const headerCols = parseCols(tableLines[0]);
      const sepCols = parseCols(tableLines[1]);
      expect(sepCols.length).toBe(headerCols.length);

      for (let i = 2; i < tableLines.length; i++) {
        const rowCols = parseCols(tableLines[i]);
        expect(rowCols.length).toBe(headerCols.length);
      }
    });
  });

  describe('4. Vault Graph & Wiki-Link Cross-References', () => {
    it('verifies all internal [[wiki-links]] point to valid vault files or targets', () => {
      const wikiLinkRegex = /\[\[([^\]|#]+)(?:#[^\]|]*)?(?:\|[^\]]*)?\]\]/g;
      let match;
      const links: string[] = [];

      while ((match = wikiLinkRegex.exec(noteContent)) !== null) {
        links.push(match[1].trim());
      }

      expect(links.length).toBeGreaterThan(5);

      // Read all filenames in vault
      const allFiles = fs.readdirSync(vaultRoot, { recursive: true })
        .map(f => String(f).replace(/\\/g, '/'))
        .filter(f => f.endsWith('.md'));

      const fileBaseNames = allFiles.map(f => path.basename(f, '.md').toLowerCase());
      const relPaths = allFiles.map(f => f.toLowerCase());
      const relPathsNoExt = allFiles.map(f => f.replace(/\.md$/, '').toLowerCase());

      for (const link of links) {
        const lower = link.toLowerCase();
        const exists = fileBaseNames.includes(lower) || relPaths.includes(lower) || relPathsNoExt.includes(lower);
        expect(exists, `Wiki link [[${link}]] should exist in vault`).toBe(true);
      }
    });
  });

  describe('5. Architectural Consistency & Contradiction Stress-Testing', () => {
    it('affirms the Unified Native Rust Runtime boundary and does NOT propose restoring retired Node/Python stack', () => {
      expect(noteContent).toContain('liva-native-core');
      expect(noteContent).toContain('Unified Native Engine in Rust');
      expect(noteContent).toMatch(/zero Node\/TS overhead/i);
      
      // Contradiction check: Ensure no instruction to revive retired Node gateway or Python AI engine
      expect(noteContent).not.toMatch(/restore (the )?(legacy )?(Node|Python) (gateway|ai-engine)/i);
      expect(noteContent).not.toMatch(/revert to Node/i);
    });

    it('enforces CommandPrincipal security model and fail-closed authorization', () => {
      expect(noteContent).toContain('CommandPrincipal');
      expect(noteContent).toContain('fail-closed');
      expect(noteContent).toContain('ToolExecPolicy');
      expect(noteContent).toMatch(/caller_principal\s*<\s*tool\.required_principal\(\)/);
    });

    it('strictly complies with SQLite WAL invariants and rejects in-memory-only event logging', () => {
      expect(noteContent).toContain('SQLite WAL');
      expect(noteContent).toMatch(/In-Memory-Only Event Log/i);
      expect(noteContent).toMatch(/Anti-Pattern/i);
      expect(noteContent).toMatch(/synchronous\s*=\s*NORMAL/i);
    });

    it('forbids blocking Tokio async control loops with synchronous I/O or heavy tool compute', () => {
      expect(noteContent).toContain('Tokio');
      expect(noteContent).toMatch(/Blocking Tokio Control Loops/i);
      expect(noteContent).toContain('spawn_blocking');
    });

    it('prevents per-token reactive DOM thrashing in Vue 3 UI', () => {
      expect(noteContent).toMatch(/Per-Token.*DOM/i);
      expect(noteContent).toContain('shallowRef');
      expect(noteContent).toContain('isThought');
    });

    it('respects VRAM and local LLM execution constraints', () => {
      expect(noteContent).toContain('llama-cpp-2');
      expect(noteContent).toContain('token_budget');
    });
  });
});
