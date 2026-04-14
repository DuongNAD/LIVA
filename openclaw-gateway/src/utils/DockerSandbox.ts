import { exec } from "child_process";
import * as util from "util";
import * as fs from "fs/promises";
import * as path from "path";
import * as crypto from "crypto";

const execAsync = util.promisify(exec);

// Global registry for active containers to ensure cleanup
const activeContainers = new Set<string>();

const cleanupAllContainers = () => {
    for (const cid of activeContainers) {
        try {
            // Using execSync because process exit must be synchronous block
            require('child_process').execSync(`docker rm -f ${cid}`);
        } catch(e) {}
    }
    activeContainers.clear();
};

process.on('exit', cleanupAllContainers);
process.on('SIGINT', () => { cleanupAllContainers(); process.exit(1); });
process.on('uncaughtException', (err) => { 
    console.error(`Uncaught exception: ${err}`);
    cleanupAllContainers(); 
    process.exit(1); 
});

export class DockerSandbox {
    private containerId: string = "";
    private workspaceSource: string;

    constructor(workspaceSource: string) {
        this.workspaceSource = workspaceSource;
    }

    getContainerId(): string {
        return this.containerId;
    }

    /**
     * Initializes the ephemeral sandbox container
     */
    async initialize() {
        const sessionId = crypto.randomBytes(4).toString("hex");
        const containerName = `liva_sandbox_${sessionId}`;
        
        // Ensure image exists
        try {
            await execAsync("docker image inspect liva-sandbox-base");
        } catch {
            console.log("[DockerSandbox] Building base image...");
            await execAsync("docker build -t liva-sandbox-base -f Dockerfile.sandbox .", { cwd: this.workspaceSource });
        }

        // Run container in detached mode with unprivileged user but root allowed to copy files
        console.log(`[DockerSandbox] Spinning up ephemeral container: ${containerName}`);
        const { stdout } = await execAsync(`docker run -d --name ${containerName} --memory=2g --cpus=2 liva-sandbox-base tail -f /dev/null`);
        this.containerId = stdout.trim();
        activeContainers.add(this.containerId);

        // Copy shadow workspace
        console.log(`[DockerSandbox] Propagating shadow workspace via docker cp...`);
        // Use a tar pipe or docker cp to copy the source without node_modules if possible, but docker cp is simpler
        // We will copy specifically src, data, tsconfig.json, package.json
        const dirs = ["src", "package.json", "tsconfig.json", "vitest.config.ts", "data"];
        for(const item of dirs) {
            const itemPath = path.join(this.workspaceSource, item);
            try {
                const stat = await fs.stat(itemPath);
                await execAsync(`docker cp ${itemPath} ${this.containerId}:/app/shadow_workspace/`);
            } catch(e) { /* ignore missing */ }
        }

        // Change ownership inside container
        await execAsync(`docker exec -u root ${this.containerId} chown -R liva_sandbox:liva_sandbox /app/shadow_workspace`);

        // Initialize git to track diffs for the Reflexion loop
        console.log(`[DockerSandbox] Tracking baseline via git init...`);
        await this.execCommand(`cd /app/shadow_workspace && git init && git config user.email "ai@liva.local" && git config user.name "AI Sandbox" && git add . && git commit -m "baseline"`);
    }

    /**
     * Executes a command inside the isolated sandbox
     */
    async execCommand(command: string, timeoutMs?: number): Promise<{ stdout: string, stderr: string }> {
        if (!this.containerId) throw new Error("Sandbox not initialized");
        return await new Promise<{ stdout: string, stderr: string }>((resolve, reject) => {
            require('child_process').execFile("docker", ["exec", "--user", "liva_sandbox", this.containerId, "sh", "-c", command], { timeout: timeoutMs || 60000 }, (error: any, stdout: string, stderr: string) => {
                if (error) {
                    error.stdout = stdout;
                    error.stderr = stderr;
                    reject(error);
                } else {
                    resolve({ stdout, stderr });
                }
            });
        });
    }

    /**
     * Copies a modified file back from sandbox to the real host
     */
    async retrieveFile(relativeFilePath: string, destPath: string) {
        if (!this.containerId) throw new Error("Sandbox not initialized");
        await execAsync(`docker cp ${this.containerId}:/app/shadow_workspace/${relativeFilePath} ${destPath}`);
    }

    /**
     * Destroys the ephemeral container completely
     */
    async destroy() {
        if (this.containerId) {
            console.log(`[DockerSandbox] Destroying ephemeral container ${this.containerId}`);
            await execAsync(`docker rm -f ${this.containerId}`).catch(() => {});
            activeContainers.delete(this.containerId);
            this.containerId = "";
        }
    }
}
