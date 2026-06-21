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
export declare function validateAndResolvePath(vaultRoot: string, inputPath: string, visited?: Set<string>): string;
export interface SearchMatch {
    lineNumber: number;
    snippet: string;
    matchedTerm: string;
}
export interface SearchResult {
    path: string;
    title: string;
    score: number;
    matches: SearchMatch[];
    frontmatter: Record<string, any>;
}
export interface DocumentIndex {
    filePath: string;
    absolutePath: string;
    title: string;
    author: string;
    tags: string[];
    frontmatter: Record<string, any>;
    lines: string[];
    lastModified: number;
}
export interface ParsedQuery {
    keywords: string[];
    fields: Record<string, string[]>;
}
export declare class QueryParser {
    private static FIELD_REGEX;
    static parse(queryText: string): ParsedQuery;
}
export declare class VaultSearchEngine {
    private vaultRoot;
    private index;
    private watcher;
    constructor(vaultRoot: string);
    /**
     * Performs initial vault scan of Skills, Knowledge, and Rules.
     */
    initialize(): void;
    private scanDirectory;
    /**
     * Parse a single markdown file and index it.
     */
    indexFile(absolutePath: string, relativePath: string): void;
    removeFile(relativePath: string): void;
    /**
     * Set up native fs.watch for incremental updates.
     */
    private setupWatcher;
    close(): void;
    /**
     * Core search runner.
     */
    search(queryText: string): SearchResult[];
}
export declare class VaultManager {
    private vaultRoot;
    private searchEngine;
    constructor(vaultRoot: string);
    getVaultRoot(): string;
    /**
     * Resolves a relative path to an absolute path within the vault.
     * Throws a permission error if the resolved path is outside the vault.
     */
    resolveSafePath(relativePath: string): string;
    /**
     * Reads a markdown file.
     */
    readMarkdown(relativePath: string): string;
    /**
     * Writes/updates a markdown file and creates parent directories if needed.
     */
    writeMarkdown(relativePath: string, content: string): boolean;
    /**
     * Searches the vault.
     */
    searchVault(query: string): SearchResult[];
    /**
     * Clean up resources (e.g. file watchers)
     */
    close(): void;
}
