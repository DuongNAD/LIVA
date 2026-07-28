import * as fs from 'fs';
import * as path from 'path';
import yaml from 'js-yaml';

/**
 * Normalizes Windows-specific paths (strips UNC prefixes, normalizes drive letter case).
 */
function cleanPath(p: string): string {
  let cleaned = path.resolve(p);
  if (process.platform === 'win32') {
    // Strip UNC prefix if present
    if (cleaned.startsWith('\\\\?\\')) {
      cleaned = cleaned.slice(4);
    }
    // Force lowercase drive letter for consistent comparison
    if (/^[a-zA-Z]:/.test(cleaned)) {
      cleaned = cleaned[0].toLowerCase() + cleaned.slice(1);
    }
  }
  return cleaned;
}

/**
 * Validates that the requested target path is safely contained within the vault directory.
 * Resolves relative paths, handles symlinks, blocks traversal, and handles OS differences.
 * 
 * @param vaultRoot Absolute path to the Obsidian Vault root.
 * @param inputPath Relative or absolute path requested by the client.
 * @param visited Set of resolved targets (used to detect circular symlinks).
 * @returns The fully resolved canonical absolute path.
 * @throws Error if the path attempts directory traversal or escapes the vault.
 */
export function validateAndResolvePath(
  vaultRoot: string,
  inputPath: string,
  visited: Set<string> = new Set()
): string {
  // 1. URL decode the input path to guard against URL-encoded traversals
  let decodedPath: string;
  try {
    decodedPath = decodeURIComponent(inputPath);
  } catch (err) {
    throw new Error("Access denied: Invalid URL-encoded path");
  }

  // 2. Reject null bytes and control characters
  if (decodedPath.includes('\0')) {
    throw new Error("Access denied: Path contains null bytes");
  }
  if (/[\x00-\x1F\x7F]/.test(decodedPath)) {
    throw new Error("Access denied: Path contains control characters");
  }

  // 3. Obtain canonical vault root path
  let canonicalVaultRoot: string;
  try {
    canonicalVaultRoot = cleanPath(fs.realpathSync(vaultRoot));
  } catch (err: any) {
    throw new Error(`Invalid vault root configuration: ${err.message}`);
  }

  // Initialize visited set with the root
  if (visited.size === 0) {
    visited.add(canonicalVaultRoot);
  }

  // 4. Resolve the target path to an absolute path
  const resolvedPath = cleanPath(path.resolve(canonicalVaultRoot, decodedPath));

  // 5. Fast-path check: verify containment using path.relative
  const relative = path.relative(canonicalVaultRoot, resolvedPath);
  const isOutside = relative.split(path.sep)[0] === '..' || path.isAbsolute(relative);
  if (isOutside) {
    throw new Error("Access denied: Path resolves outside the vault directory");
  }

  // 6. Deep verification: process each segment to resolve and validate symlinks
  const segments = relative.split(path.sep).filter(s => s !== '' && s !== '.');
  let currentPath = canonicalVaultRoot;

  for (const segment of segments) {
    const nextPath = path.join(currentPath, segment);

    let stat: fs.Stats | null = null;
    try {
      stat = fs.lstatSync(nextPath);
    } catch (err: any) {
      if (err.code === 'ENOENT') {
        // Segment does not exist on disk.
        // Nonexistent segments cannot be symlinks or escape.
        // We can safely assume the remaining path is valid.
        currentPath = nextPath;
        continue;
      }
      throw err;
    }

    if (stat.isSymbolicLink()) {
      // Resolve symlink target
      let target = fs.readlinkSync(nextPath);

      // If symlink target is relative, resolve it relative to the symlink's directory
      if (!path.isAbsolute(target)) {
        target = path.resolve(currentPath, target);
      } else {
        target = path.resolve(target);
      }

      const canonicalTarget = cleanPath(target);

      // Check for circular symlink loops
      if (visited.has(canonicalTarget)) {
        throw new Error("Access denied: Symlink loop detected");
      }
      visited.add(canonicalTarget);

      // Recursively validate target path to handle nested symlinks and escapes
      currentPath = validateAndResolvePath(canonicalVaultRoot, canonicalTarget, visited);
    } else {
      currentPath = nextPath;
    }
  }

  // 7. Final double-check on fully resolved canonical path
  const finalRelative = path.relative(canonicalVaultRoot, currentPath);
  const finalOutside = finalRelative.split(path.sep)[0] === '..' || path.isAbsolute(finalRelative);
  if (finalOutside) {
    throw new Error("Access denied: Path resolves outside the vault directory");
  }

  return currentPath;
}

export interface SearchMatch {
  lineNumber: number;    // 1-based line number in the original file
  snippet: string;       // Trimmed line text
  matchedTerm: string;   // The specific query term that matched
}

export interface SearchResult {
  path: string;                      // Relative path from vault root
  title: string;                     // Title from frontmatter
  score: number;                     // Relevancy score
  matches: SearchMatch[];            // Snippets of matching lines
  frontmatter: Record<string, any>;  // All frontmatter properties
}

export interface DocumentIndex {
  filePath: string;                  // Relative path (key)
  absolutePath: string;              // Absolute path on disk
  title: string;
  author: string;
  tags: string[];
  frontmatter: Record<string, any>;
  lines: string[];                   // Original lines of the file
  lastModified: number;              // mtime timestamp
}

export interface ParsedQuery {
  keywords: string[];                  // e.g. ["architecture", "safety"]
  fields: Record<string, string[]>;    // e.g. { tags: ["liva/knowledge"], author: ["explorer"] }
}

export class QueryParser {
  private static FIELD_REGEX = /(\w+):(?:([^"\s]+)|"([^"]+)")/g;

  public static parse(queryText: string): ParsedQuery {
    const keywords: string[] = [];
    const fields: Record<string, string[]> = {};

    let match;
    const cleanText = queryText.trim();

    this.FIELD_REGEX.lastIndex = 0;
    while ((match = this.FIELD_REGEX.exec(cleanText)) !== null) {
      const fieldName = match[1].toLowerCase();
      const fieldValue = (match[2] || match[3]).toLowerCase();

      if (!fields[fieldName]) {
        fields[fieldName] = [];
      }
      fields[fieldName].push(fieldValue);
    }

    // Remove field terms to extract raw keywords
    this.FIELD_REGEX.lastIndex = 0;
    const keywordText = cleanText.replace(this.FIELD_REGEX, '').trim();
    if (keywordText) {
      // Split by whitespace and filter empty strings
      keywords.push(...keywordText.split(/\s+/).map(k => k.toLowerCase()));
    }

    return { keywords, fields };
  }
}

export class VaultSearchEngine {
  private vaultRoot: string;
  private index: Map<string, DocumentIndex> = new Map();
  private watcher: fs.FSWatcher | null = null;

  constructor(vaultRoot: string) {
    this.vaultRoot = vaultRoot;
  }

  /**
   * Performs initial vault scan of Skills, Knowledge, and Rules.
   */
  public initialize(): void {
    const targetDirs = ['Skills', 'Knowledge', 'Rules'];
    for (const dir of targetDirs) {
      const dirPath = path.join(this.vaultRoot, dir);
      if (fs.existsSync(dirPath)) {
        this.scanDirectory(dirPath, dir);
      }
    }
    this.setupWatcher();
  }

  private scanDirectory(dirPath: string, subFolder: string): void {
    const entries = fs.readdirSync(dirPath, { withFileTypes: true });
    for (const entry of entries) {
      const fullPath = path.join(dirPath, entry.name);
      const relPath = `${subFolder}/${entry.name}`;
      if (entry.isDirectory()) {
        this.scanDirectory(fullPath, relPath);
      } else if (entry.isFile() && entry.name.endsWith('.md')) {
        this.indexFile(fullPath, relPath);
      }
    }
  }

  /**
   * Parse a single markdown file and index it.
   */
  public indexFile(absolutePath: string, relativePath: string): void {
    try {
      const stat = fs.statSync(absolutePath);
      const content = fs.readFileSync(absolutePath, 'utf8');
      const lines = content.split(/\r?\n/);

      // Parse YAML frontmatter using regex
      const frontmatterRegex = /^---\r?\n([\s\S]*?)\r?\n---(?:\r?\n|$)/;
      const fmMatch = content.match(frontmatterRegex);

      let frontmatter: Record<string, any> = {};
      if (fmMatch) {
        try {
          frontmatter = (yaml.load(fmMatch[1]) as Record<string, any>) || {};
        } catch (e) {
          console.warn(`Failed to parse frontmatter for ${relativePath}:`, e);
        }
      }

      this.index.set(relativePath, {
        filePath: relativePath,
        absolutePath,
        title: String(frontmatter.title || path.basename(relativePath, '.md')),
        author: String(frontmatter.author || ''),
        tags: Array.isArray(frontmatter.tags) ? frontmatter.tags.map(t => String(t)) : [],
        frontmatter,
        lines,
        lastModified: stat.mtimeMs,
      });
    } catch (err) {
      console.error(`Error indexing file ${relativePath}:`, err);
    }
  }

  public removeFile(relativePath: string): void {
    this.index.delete(relativePath);
  }

  /**
   * Set up native fs.watch for incremental updates.
   */
  private setupWatcher(): void {
    try {
      this.watcher = fs.watch(this.vaultRoot, { recursive: true }, (eventType, filename) => {
        if (!filename || !filename.endsWith('.md')) return;
        
        // Normalize slashes
        const normalizedRelPath = filename.replace(/\\/g, '/');
        
        // Ignore Templates
        if (normalizedRelPath.startsWith('Templates/')) return;

        const fullPath = path.join(this.vaultRoot, filename);
        if (fs.existsSync(fullPath)) {
          this.indexFile(fullPath, normalizedRelPath);
        } else {
          this.removeFile(normalizedRelPath);
        }
      });
    } catch (e) {
      console.warn('File watcher not supported or failed to initialize:', e);
    }
  }

  public close(): void {
    if (this.watcher) {
      this.watcher.close();
    }
  }

  /**
   * Core search runner.
   */
  public search(queryText: string): SearchResult[] {
    const query = QueryParser.parse(queryText);
    const results: SearchResult[] = [];
    const statusFilterRequested = Object.prototype.hasOwnProperty.call(query.fields, 'status');

    for (const doc of this.index.values()) {
      const status = String(doc.frontmatter.status ?? '').toLowerCase();
      if (status === 'archived' && !statusFilterRequested) continue;

      // 1. Apply field-specific filters (e.g. tags:x, author:y, title:z)
      let fieldMatch = true;
      for (const [field, values] of Object.entries(query.fields)) {
        if (field === 'tags') {
          // Check if document tags contain any of the filter values (loose check)
          const docTags = doc.tags.map(t => t.toLowerCase());
          const matchAnyTag = values.some(val => docTags.some(dt => dt.includes(val)));
          if (!matchAnyTag) {
            fieldMatch = false;
            break;
          }
        } else if (field === 'author') {
          const docAuthor = doc.author.toLowerCase();
          const matchAuthor = values.some(val => docAuthor.includes(val));
          if (!matchAuthor) {
            fieldMatch = false;
            break;
          }
        } else if (field === 'title') {
          const docTitle = doc.title.toLowerCase();
          const matchTitle = values.some(val => docTitle.includes(val));
          if (!matchTitle) {
            fieldMatch = false;
            break;
          }
        } else {
          // Custom frontmatter fields
          const val = doc.frontmatter[field];
          if (val === undefined) {
            fieldMatch = false;
            break;
          }
          const strVal = String(val).toLowerCase();
          const matchCustom = values.some(qv => strVal.includes(qv));
          if (!matchCustom) {
            fieldMatch = false;
            break;
          }
        }
      }

      if (!fieldMatch) continue;

      // 2. Keyword Matching & Scoring
      let score = 0;
      const matches: SearchMatch[] = [];

      if (query.keywords.length > 0) {
        // Boost scores if keywords match frontmatter metadata
        for (const keyword of query.keywords) {
          if (doc.title.toLowerCase().includes(keyword)) score += 10;
          if (doc.author.toLowerCase().includes(keyword)) score += 5;
          if (doc.tags.some(t => t.toLowerCase().includes(keyword))) score += 8;

          // Full text search in the file lines (frontmatter + body)
          doc.lines.forEach((lineText, idx) => {
            if (lineText.toLowerCase().includes(keyword)) {
              score += 1;
              matches.push({
                lineNumber: idx + 1,
                snippet: lineText.trim(),
                matchedTerm: keyword
              });
            }
          });
        }
      } else {
        // If there are no keyword searches, but we filtered on fields, assign base score
        score = 1;
      }

      if (score > 0) {
        results.push({
          path: doc.filePath,
          title: doc.title,
          score,
          matches,
          frontmatter: doc.frontmatter,
        });
      }
    }

    // Sort by relevancy score descending
    return results.sort((a, b) => b.score - a.score);
  }
}

export class VaultManager {
  private vaultRoot: string;
  private searchEngine: VaultSearchEngine;

  constructor(vaultRoot: string) {
    if (!vaultRoot) {
      throw new Error("Vault root path must be specified");
    }
    let canonicalRoot: string;
    try {
      canonicalRoot = cleanPath(fs.realpathSync(vaultRoot));
    } catch (err: any) {
      throw new Error(`Vault root path does not exist or is invalid: ${vaultRoot}`);
    }
    this.vaultRoot = canonicalRoot;
    this.searchEngine = new VaultSearchEngine(this.vaultRoot);
    this.searchEngine.initialize();
  }

  public getVaultRoot(): string {
    return this.vaultRoot;
  }

  /**
   * Resolves a relative path to an absolute path within the vault.
   * Throws a permission error if the resolved path is outside the vault.
   */
  public resolveSafePath(relativePath: string): string {
    return validateAndResolvePath(this.vaultRoot, relativePath);
  }

  /**
   * Reads a markdown file.
   */
  public readMarkdown(relativePath: string): string {
    const safePath = this.resolveSafePath(relativePath);
    if (!fs.existsSync(safePath)) {
      throw new Error(`File not found: ${relativePath}`);
    }
    if (!fs.statSync(safePath).isFile()) {
      throw new Error(`Path is not a file: ${relativePath}`);
    }
    return fs.readFileSync(safePath, 'utf8');
  }

  /**
   * Writes/updates a markdown file and creates parent directories if needed.
   */
  public writeMarkdown(relativePath: string, content: string): boolean {
    const safePath = this.resolveSafePath(relativePath);
    const parentDir = path.dirname(safePath);

    // Automatically create directories if they do not exist
    if (!fs.existsSync(parentDir)) {
      fs.mkdirSync(parentDir, { recursive: true });
    }

    fs.writeFileSync(safePath, content, 'utf8');

    // Manually trigger indexing to ensure sync search results immediately
    const cleanRelPath = path.relative(this.vaultRoot, safePath).replace(/\\/g, '/');
    if (!cleanRelPath.startsWith('Templates/')) {
      this.searchEngine.indexFile(safePath, cleanRelPath);
    }

    return true;
  }

  /**
   * Searches the vault.
   */
  public searchVault(query: string): SearchResult[] {
    return this.searchEngine.search(query);
  }

  /**
   * Clean up resources (e.g. file watchers)
   */
  public close(): void {
    this.searchEngine.close();
  }
}
