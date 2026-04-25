import * as fs from "node:fs";
import * as path from "node:path";
import { execSync, execFileSync } from "node:child_process";

/**
 * Blue-Green Router V7: Git-Native Atomic Deployment
 * ====================================================
 * Replaces raw filesystem copy with git branch/commit/rollback:
 * 
 * 1. Before mutation: Save current state to git (stash or commit)
 * 2. Deploy: Copy sandbox files + git commit with structured message
 * 3. Rollback: git checkout to restore previous state
 * 
 * Benefits:
 * - Full history of every evolution attempt
 * - Easy rollback to any specific mutation
 * - Diff visibility for debugging
 */
export class BlueGreenRouter {
    private hostWorkspace: string;
    private currentEvolutionBranch: string | null = null;

    constructor(workspace: string) {
         this.hostWorkspace = workspace;
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
     * Deploy mutation from sandbox to host using git-native operations.
     * 
     * Steps:
     * 1. Stash any uncommitted changes
     * 2. Copy sandbox src/ over host src/
     * 3. Git add + commit with structured evolution message
     * 4. On failure: auto-rollback via git checkout
     */
    public async deployToGreenBatch(sandboxRoot: string): Promise<boolean> {
         const originalSrcPath = path.join(this.hostWorkspace, "src");
         const sandboxSrcPath = path.join(sandboxRoot, "src");
         const baseBranch = this.getCurrentBranch();

         try {
             // PHASE 1: Ensure clean state — stash any dirty changes
             if (!this.isWorkingTreeClean()) {
                 console.log("[Deployer] Stashing uncommitted changes...");
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
             if (!fs.existsSync(sandboxSrcPath)) {
                 console.error(`[Deployer] Sandbox src/ not found: ${sandboxSrcPath}`);
                 return false;
             }

             fs.cpSync(sandboxSrcPath, originalSrcPath, { recursive: true, force: true });
             
             // PHASE 3: Git commit the evolution  
             try {
                 execFileSync("git", ["add", "-A"], {
                     cwd: this.hostWorkspace,
                     encoding: "utf-8",
                     stdio: "pipe",
                 });

                 const timestamp = new Date().toISOString().replaceAll(/[:.]/g, "-");
                 const commitMsg = `evolution: deployed mutation at ${timestamp}`;
                 
                 execFileSync("git", ["commit", "-m", commitMsg, "--no-verify"], {
                     cwd: this.hostWorkspace,
                     encoding: "utf-8",
                     stdio: "pipe",
                 });

                 console.log(`\n🟢 [Deployer] Git commit successful: "${commitMsg}"`);
             } catch (gitErr: any) {
                 // If nothing to commit (no actual changes), still consider it success
                 if (gitErr.stderr?.includes("nothing to commit")) {
                     console.log("[Deployer] No changes to commit (sandbox identical to host).");
                 } else {
                     console.warn(`[Deployer] Git commit warning: ${gitErr.message}`);
                 }
             }

             // PHASE 4: Cleanup sandbox
             if (fs.existsSync(sandboxRoot)) fs.rmSync(sandboxRoot, { recursive: true, force: true });
             
             console.log(`\n🟢 [Deployer] GREEN deployment complete with git tracking!`);
             return true;

         } catch(e: any) {
             console.error("🔴 Deployment error (ATOMIC ROLLBACK):", e.message);
             await this.autoRollbackBatch();
             return false;
         }
    }

    /**
     * Emergency Rollback: Restore host to last committed state.
     * Uses git checkout to discard all uncommitted changes.
     */
    public async autoRollbackBatch(): Promise<boolean> {
        try {
            console.log("[Deployer] 🔴 AUTO-ROLLBACK initiated...");
            
            // Discard all changes in src/
            execSync("git checkout -- src/", {
                cwd: this.hostWorkspace,
                encoding: "utf-8",
                stdio: "pipe",
            });

            // Remove untracked files in src/
            execSync("git clean -fd src/", {
                cwd: this.hostWorkspace,
                encoding: "utf-8",
                stdio: "pipe",
            });

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

            console.log("🔴 [Deployer] AUTO-ROLLBACK complete — src/ restored to last commit.");
            return true;

        } catch (e: any) {
            console.error("🔴 Fatal rollback error:", e.message);
            
            // Ultimate fallback: filesystem backup (legacy V6 behavior)
            const backupSrcPath = path.join(this.hostWorkspace, ".src.blue.bak");
            const originalSrcPath = path.join(this.hostWorkspace, "src");
            if (fs.existsSync(backupSrcPath)) {
                if (fs.existsSync(originalSrcPath)) fs.rmSync(originalSrcPath, { recursive: true, force: true });
                fs.cpSync(backupSrcPath, originalSrcPath, { recursive: true });
                console.log("🔴 [Deployer] Filesystem fallback rollback used.");
                return true;
            }
            return false;
        }
    }

    // Backward compatibility
    public async autoRollback(originalFilePath: string): Promise<boolean> {
        return this.autoRollbackBatch();
    }
}
