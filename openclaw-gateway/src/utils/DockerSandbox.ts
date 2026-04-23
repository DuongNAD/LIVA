import { exec, execSync, spawn } from "child_process";
import * as util from "util";
import * as fs from "fs/promises";
import * as path from "path";
import * as crypto from "crypto";
import kill from "tree-kill";
import { logger } from "./logger";

const execAsync = util.promisify(exec);

// Global registry for active containers to ensure cleanup
const activeContainers = new Set<string>();

const cleanupAllContainers = () => {
    for (const cid of activeContainers) {
        try {
            // Using execSync because process exit must be synchronous block
            execSync(`docker rm -f ${cid}`);
        } catch(e) {}
    }
    activeContainers.clear();
};

process.on('exit', cleanupAllContainers);
process.on('SIGINT', () => { cleanupAllContainers(); process.exit(1); });
process.on('uncaughtException', (err) => { 
    logger.error(`Uncaught exception: ${err}`);
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
        const containerName = `liva_sandbox_daemon`;
        
        // Auto-Wake Docker Desktop if offline
        logger.info("[DockerSandbox] Checking Docker Daemon status...");
        try {
            await execAsync("docker info");
        } catch (e) {
            logger.info("[DockerSandbox] Docker Daemon is offline. Attempting to wake up Docker Desktop...");
            try {
                await execAsync('Start-Process "C:\\Program Files\\Docker\\Docker\\Docker Desktop.exe"', { shell: "powershell.exe" });
                let isDockerAwake = false;
                for(let i = 0; i < 40; i++) {
                    try {
                        await execAsync("docker info");
                        isDockerAwake = true;
                        logger.info("[DockerSandbox] Docker is awake and ready!");
                        break;
                    } catch(err) {
                        process.stdout.write(".");
                        await new Promise(r => setTimeout(r, 2000));
                    }
                }
                if (!isDockerAwake) throw new Error("Cannot wake up Docker Desktop. Timeout after 80s.");
            } catch (startErr) {
                logger.error("[DockerSandbox] Failed to start Docker Desktop. Please start it manually.");
                throw startErr;
            }
        }

        try {
            await execAsync("docker image inspect liva-sandbox-base");
        } catch {
            logger.info("[DockerSandbox] Building base image...");
            await execAsync("docker build -t liva-sandbox-base -f Dockerfile.sandbox .", { cwd: this.workspaceSource });
        }

        // Long-running sandbox: check if already exists
        try {
            const { stdout } = await execAsync(`docker ps -a -q -f name=${containerName}`);
            if (stdout.trim().length > 0) {
                 await execAsync(`docker rm -f ${containerName}`);
            }
        } catch(e) {}

        const hostShadowPath = path.join(this.workspaceSource, "../shadow_workspace");
        await fs.mkdir(hostShadowPath, { recursive: true });

        logger.info(`[DockerSandbox] Spinning up Long-running Daemon: ${containerName}`);
        // Pure Linux Isolated Environment - No bind mounts to save Disk I/O
        // V25: Removed hardcoded --memory=2g --cpus=2 to prevent Docker Daemon crashing during 4GB TypeScript compilation.
        const { stdout } = await execAsync(`docker run -d --name ${containerName} liva-sandbox-base tail -f /dev/null`);
        this.containerId = stdout.trim();
        activeContainers.add(this.containerId);

        // Ensure directory exists in container before copy
        await execAsync(`docker exec -u root ${this.containerId} mkdir -p /app/shadow_workspace`);

        // Copy shadow workspace
        logger.info(`[DockerSandbox] Propagating shadow workspace via docker cp...`);
        // Use a tar pipe or docker cp to copy the source without node_modules if possible, but docker cp is simpler
        // We will copy specifically src, data, tsconfig.json, package.json
        const dirs = ["src", "package.json", "tsconfig.json", "vitest.config.ts", "data", ".gitignore"];
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
        logger.info(`[DockerSandbox] Tracking baseline via git init...`);
        
        // V10 Fix: Install node modules inside Linux sandbox to prevent "Cannot find module 'dotenv'" Error
        logger.info(`[DockerSandbox] Running npm install inside sandbox (Fast Cache)...`);
        await this.execCommand(`cd /app/shadow_workspace && npm install --no-audit --no-fund --legacy-peer-deps`, 300000).catch(()=>true);

        // V18 Fix: Chống tràn RAM (OOM Exit 137) do git add . cố gắng tính toán băm node_modules khổng lồ.
        await this.execCommand(`echo "node_modules" > /app/shadow_workspace/.gitignore`);

        await this.execCommand(`cd /app/shadow_workspace && git config --global --add safe.directory /app/shadow_workspace && git init && git config user.email "ai@liva.local" && git config user.name "AI Sandbox" && git config --global init.defaultBranch main && git add . && git commit -m "baseline"`);
    }

    disconnectNetwork() {
        try { execSync(`docker network disconnect bridge ${this.containerId}`, { stdio: 'ignore' }); } catch(e) {}
    }

    connectNetwork() {
        try { execSync(`docker network connect bridge ${this.containerId}`, { stdio: 'ignore' }); } catch(e) {}
    }

    async resetSandboxState() {
        await this.execCommand('cd /app/shadow_workspace && git reset --hard HEAD && git clean -fd');
    }

    async commitCheckpoint() {
        await this.execCommand('cd /app/shadow_workspace && git add . && git commit -m "Auto-evolution success"');
    }

    async execCommand(command: string, timeoutMs: number = 60000): Promise<{ stdout: string, stderr: string }> {
        if (!this.containerId) throw new Error("Sandbox not initialized");
        return await new Promise<{ stdout: string, stderr: string }>((resolve, reject) => {
            let stdout = "";
            let stderr = "";
            
            const child = spawn("docker", ["exec", "--user", "liva_sandbox", this.containerId, "sh", "-c", command], { detached: true });
            
            child.stdout?.on('data', (d: any) => stdout += d.toString());
            child.stderr?.on('data', (d: any) => stderr += d.toString());
            
            let isTimeout = false;
            const timer = setTimeout(() => {
                isTimeout = true;
                // V10 Cross-OS Zombie Killer
                if (child.pid) kill(child.pid, 'SIGKILL');
                try { exec(`docker exec ${this.containerId} pkill -9 -f vitest`); } catch(e){}
                reject(new Error(`Timeout ${timeoutMs}ms exceeded. Process truncated and pkill sent to Linux.`));
            }, timeoutMs);

            child.on('close', (code: number) => {
                clearTimeout(timer);
                if (isTimeout) return;
                
                if (code !== 0) {
                    const err: any = new Error(`Command failed with exit code ${code}\nStderr: ${stderr.trim()}\nStdout: ${stdout.trim()}`);
                    err.stdout = stdout;
                    err.stderr = stderr;
                    reject(err);
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
     * Copies a file from host to the sandbox
     */
    async pushFile(hostFilePath: string, relativeFilePath: string) {
        if (!this.containerId) throw new Error("Sandbox not initialized");
        await execAsync(`docker cp ${hostFilePath} ${this.containerId}:/app/shadow_workspace/${relativeFilePath}`);
    }

    /**
     * Destroys the ephemeral container completely
     */
    async destroy() {
        if (this.containerId) {
            logger.info(`[DockerSandbox] Destroying ephemeral container ${this.containerId}`);
            await execAsync(`docker rm -f ${this.containerId}`).catch(() => {});
            activeContainers.delete(this.containerId);
            this.containerId = "";
        }
    }
}
