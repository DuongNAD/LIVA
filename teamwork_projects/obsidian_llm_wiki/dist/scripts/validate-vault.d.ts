interface FileValidationResult {
    filePath: string;
    relativeDocPath: string;
    isValid: boolean;
    errors: string[];
    warnings: string[];
    isTemplate: boolean;
}
interface VaultValidationReport {
    totalFilesChecked: number;
    invalidFilesCount: number;
    brokenLinksCount: number;
    errors: string[];
    fileResults: FileValidationResult[];
}
/**
 * Validates the Obsidian Vault structure, template files, frontmatter schemas, and wiki links.
 *
 * @param vaultRoot Absolute path to the Obsidian Vault root directory
 */
export declare function validateVault(vaultRoot: string): VaultValidationReport;
export {};
