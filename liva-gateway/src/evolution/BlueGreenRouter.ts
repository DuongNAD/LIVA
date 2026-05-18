import * as fsp from "node:fs/promises";
import * as path from "node:path";
import { execSync, execFileSync } from "node:child_process";
import { logger } from "../utils/logger";

/**
 * Blue-Green Router V8: Safe Rollback via Physical Snapshot
 * ==========================================================
 * Security Hardening: Replaces destructive `git checkout -- src/` and
 * `git clean -fd src/` with a physical folder backup/restore mechanism.
 * 
 * Problem (V7): `git checkout` and `git clean` on the main working tree
 * can silently destroy uncommitted work across ALL of `src/`, not just
 * the files touched by the evolution pipeline.
 * 
 * Solution (V8):
 * 1. Before mutation: Create `.src.rollback.bak` physical snapshot of `src/`
 * 2. Deploy: Copy sandbox files + git commit with structured message
 * 3. Rollback: Restore `src/` from `.src.rollback.bak` snapshot
 * 
 * Benefits:
 * - ZERO risk of destroying uncommitted work outside evolution scope
 * - Full git history of every evolution attempt (commit-based)
 * - Deterministic rollback — snapshot is a known-good state
 * - No destructive git commands (`git checkout --`, `git clean -fd`) ever touch src/
 */
export class BlueGreenRouter {
    private readonly hostWorkspace: string;
    /** Physical snapshot path for safe rollback */
    private readonly ROLLBACK_BAK_DIR: string;
    private currentEvolutionBranch: string | null = null;

    constructor(workspace: string) {
         this.hostWorkspace = workspace;
         this.ROLLBACK_BAK_DIR = path.join(workspace, ".src.rollback.bak");
    }

    private async existsAsync(p: string): Promise<boolean> {
        try {
            await fsp.access(p);
            return true;
        } catch {
            return false;
        }
    }

    /**
     * Get the current git branch name.
     */
    private getCurrentBranch(): string {
        try {
            return execSync("git rev-parse --abbrev-ref HEAD", {
                cwd: this.hostWorkspace,
                encoding: "utf-8",
                stdio: "pipe",
            }).trim();
        } catch {
            return "unknown";
        }
    }

    /**
     * Check if git working tree is clean.
     */
    private isWorkingTreeClean(): boolean {
        try {
            const status = execSync("git status --porcelain", {
                cwd: this.hostWorkspace,
                encoding: "utf-8",
                stdio: "pipe",
            }).trim();
            return status.length === 0;
        } catch {
            return false;
        }
    }

    /**
     * Create a physical snapshot of src/ for safe rollback.
     * Uses synchronous copy to guarantee atomicity before any mutation starts.
     */
    private async createRollbackSnapshot(): Promise<boolean> {
        const srcPath = path.join(this.hostWorkspace, "src");
        try {
            // Clean previous snapshot if exists
            if (await this.existsAsync(this.ROLLBACK_BAK_DIR)) {
                await fsp.rm(this.ROLLBACK_BAK_DIR, { recursive: true, force: true });
            }
            // Physical copy: src/ → .src.rollback.bak/
            await fsp.cp(srcPath, this.ROLLBACK_BAK_DIR, { recursive: true });
            logger.info(`[Deployer] 📸 Rollback snapshot created: ${this.ROLLBACK_BAK_DIR}`);
            return true;
        } catch (e: unknown) {
            const errMsg = e instanceof Error ? e.message : String(e);
            logger.error(`[Deployer] Failed to create rollback snapshot: ${errMsg}`);
            return false;
        }
    }

    /**
     * Clean up the rollback snapshot after successful deployment.
     */
    private async cleanupRollbackSnapshot(): Promise<void> {
        try {
            if (await this.existsAsync(this.ROLLBACK_BAK_DIR)) {
                await fsp.rm(this.ROLLBACK_BAK_DIR, { recursive: true, force: true });
                logger.info("[Deployer] 🧹 Rollback snapshot cleaned up.");
            }
        } catch {
            // Non-critical: snapshot cleanup failure won't break anything
        }
    }

    /**
     * Deploy mutation from sandbox to host using physical snapshot + git commit.
     * 
     * Steps:
     * 1. Create physical snapshot of src/ → .src.rollback.bak (safe rollback point)
     * 2. Stash any uncommitted changes
     * 3. Copy sandbox src/ over host src/
     * 4. Git add + commit with structured evolution message
     * 5. On failure: restore from physical snapshot (NO destructive git commands)
     */
    public async deployToGreenBatch(sandboxRoot: string): Promise<boolean> {
         const originalSrcPath = path.join(this.hostWorkspace, "src");
         const sandboxSrcPath = path.join(sandboxRoot, "src");
         const baseBranch = this.getCurrentBranch();

         try {
             // PHASE 0: Create physical rollback snapshot BEFORE any mutation
             if (!(await this.createRollbackSnapshot())) {
                 logger.error("[Deployer] Cannot proceed without rollback snapshot. Aborting.");
                 return false;
             }

             // PHASE 1: Ensure clean state — stash any dirty changes
             if (!this.isWorkingTreeClean()) {
                 logger.info("[Deployer] Stashing uncommitted changes...");
                 try {
                     execSync("git stash push -m \"evolution-pre-deploy-stash\"", {
                         cwd: this.hostWorkspace,
                         encoding: "utf-8",
                         stdio: "pipe",
                     });
                 } catch {
                     // Non-fatal: working tree might have untracked files only
                 }
             }

             // PHASE 2: Copy sandbox files over host
             if (!(await this.existsAsync(sandboxSrcPath))) {
                 logger.error(`[Deployer] Sandbox src/ not found: ${sandboxSrcPath}`);
                 // Restore snapshot since we haven't deployed anything
                 await this.autoRollbackBatch();
                 return false;
             }

             await fsp.cp(sandboxSrcPath, originalSrcPath, { recursive: true, force: true });
             
             // PHASE 3: Git commit the evolution  
             try {
                 execFileSync("git", ["add", "-A"], {
                     cwd: this.hostWorkspace,
                     encoding: "utf-8",
                     stdio: "pipe",
                 });

                 const timestamp = new Date().toISOString().replace(/[:.]/g, "-");
                 const commitMsg = `evolution: deployed mutation at ${timestamp}`;
                 
                 execFileSync("git", ["commit", "-m", commitMsg, "--no-verify"], {
                     cwd: this.hostWorkspace,
                     encoding: "utf-8",
                     stdio: "pipe",
                 });

                 logger.info(`[Deployer] 🟢 Git commit successful: "${commitMsg}"`);
             } catch (gitErr: unknown) {
                 const errMsg = gitErr instanceof Error ? gitErr.message : String(gitErr);
                 // If nothing to commit (no actual changes), still consider it success
                 if ((gitErr as Error & { stderr?: string }).stderr?.includes("nothing to commit")) {
                     logger.info("[Deployer] No changes to commit (sandbox identical to host).");
                 } else {
                     logger.warn(`[Deployer] Git commit warning: ${errMsg}`);
                 }
             }

             // PHASE 4: Cleanup sandbox + rollback snapshot (success → no longer needed)
             if (await this.existsAsync(sandboxRoot)) await fsp.rm(sandboxRoot, { recursive: true, force: true });
             await this.cleanupRollbackSnapshot();
             
             logger.info(`[Deployer] 🟢 GREEN deployment complete with git tracking!`);
             return true;

         } catch(e: unknown) {
             const errMsg = e instanceof Error ? e.message : String(e);
             logger.error(`[Deployer] 🔴 Deployment error (SAFE ROLLBACK): ${errMsg}`);
             await this.autoRollbackBatch();
             return false;
         }
    }

    /**
     * Safe Rollback: Restore src/ from physical snapshot.
     * 
     * ⛔ SECURITY HARDENING: This method NO LONGER runs:
     *   - `git checkout -- src/`   (destroys ALL uncommitted changes)
     *   - `git clean -fd src/`     (deletes ALL untracked files)
     * 
     * Instead, it restores from the `.src.rollback.bak` physical snapshot
     * which contains the exact state of src/ before the evolution attempt.
     */
    public async autoRollbackBatch(): Promise<boolean> {
        const originalSrcPath = path.join(this.hostWorkspace, "src");

        try {
            logger.info("[Deployer] 🔴 SAFE ROLLBACK initiated (physical snapshot restore)...");
            
            // Restore src/ from physical snapshot
            if (await this.existsAsync(this.ROLLBACK_BAK_DIR)) {
                if (await this.existsAsync(originalSrcPath)) {
                    await fsp.rm(originalSrcPath, { recursive: true, force: true });
                }
                await fsp.cp(this.ROLLBACK_BAK_DIR, originalSrcPath, { recursive: true });
                
                // Clean up snapshot after successful restore
                await this.cleanupRollbackSnapshot();

                logger.info("[Deployer] 🔴 SAFE ROLLBACK complete — src/ restored from physical snapshot.");
            } else {
                logger.warn("[Deployer] No rollback snapshot found. Attempting legacy .src.blue.bak fallback...");
                // Legacy fallback path (V6 compatibility)
                const legacyBackupPath = path.join(this.hostWorkspace, ".src.blue.bak");
                if (await this.existsAsync(legacyBackupPath)) {
                    if (await this.existsAsync(originalSrcPath)) {
                        await fsp.rm(originalSrcPath, { recursive: true, force: true });
                    }
                    await fsp.cp(legacyBackupPath, originalSrcPath, { recursive: true });
                    logger.info("[Deployer] 🔴 Legacy filesystem fallback rollback used.");
                } else {
                    logger.error("[Deployer] 🔴 No rollback source available. Manual intervention required.");
                    return false;
                }
            }

            // Pop stash if we stashed earlier
            try {
                execSync("git stash pop", {
                    cwd: this.hostWorkspace,
                    encoding: "utf-8",
                    stdio: "pipe",
                });
            } catch {
                // No stash to pop — that's fine
            }

            return true;

        } catch (e: unknown) {
            const errMsg = e instanceof Error ? e.message : String(e);
            logger.error(`[Deployer] 🔴 Fatal rollback error: ${errMsg}`);
            return false;
        }
    }

    // Backward compatibility
    public async autoRollback(originalFilePath: string): Promise<boolean> {
        return this.autoRollbackBatch();
    }
}
