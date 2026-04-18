import { spawn, ChildProcess } from 'child_process';
import { promisify } from 'util';
import { exec } from 'child_process';
import * as path from 'path';

const execAsync = promisify(exec);

/**
 * LIVA IMMUTABLE WATCHDOG
 * Dual-Process Architecture: Monitors auto_singularity.ts and performs
 * Git Rollback and LanceDB Snapshot Rollback if the Daemon crashes.
 */

let daemonProcess: ChildProcess | null = null;
let isShuttingDown = false;

async function rollbackSystem() {
    console.log(`\x1b[31m[WATCHDOG DANGER] Singularity Daemon crashed or exited abnormally! Triggering Rollback Mechanism!\x1b[0m`);
    try {
        console.log(`\n\x1b[35m[WATCHDOG] 1. Executing Git Rollback (git reset --hard HEAD~1)...\x1b[0m`);
        await execAsync(`git reset --hard HEAD~1`);

        console.log(`\x1b[35m[WATCHDOG] 2. Re-triggering NPM install to match package.json of baseline...\x1b[0m`);
        await execAsync(`npm install`);

        console.log(`\x1b[35m[WATCHDOG] 3. Executing LanceDB Snapshot Sync (Purging dirty vectors)...\x1b[0m`);
        // We trigger a utility script inside the project safely to purge unexpected LanceDB records
        // Using tsx directly since we know it's a TS project
        await execAsync(`npx tsx src/utils/LanceDBSnapshotRollback.ts`);

        console.log(`\x1b[32m[WATCHDOG] Rollback Complete. System is restored to Immutable Baseline.\x1b[0m`);
    } catch (e: any) {
        console.log(`\x1b[31m[WATCHDOG FATAL] Rollback failed! Manual intervention required! Error: ${e.message}\x1b[0m`);
    }
}

function startDaemon() {
    console.log(`\x1b[36m[WATCHDOG] Spawning Singularity Daemon (auto_singularity.ts)...\x1b[0m`);
    
    daemonProcess = spawn('npx', ['tsx', 'src/auto_singularity.ts'], {
        stdio: 'inherit',
        shell: true
    });

    daemonProcess.on('exit', async (code) => {
        if (isShuttingDown) return;
        
        console.log(`\x1b[33m[WATCHDOG] Daemon exited with code ${code}\x1b[0m`);
        
        if (code !== 0) {
            // Abnormal crash! 
            await rollbackSystem();
        }
        
        console.log(`\x1b[36m[WATCHDOG] Restarting Daemon in 5 seconds...\x1b[0m`);
        setTimeout(startDaemon, 5000);
    });

    daemonProcess.on('error', (err) => {
        console.log(`\x1b[31m[WATCHDOG] Failed to spawn Daemon: ${err.message}\x1b[0m`);
    });
}

// Handle graceful shutdown of Watchdog itself
process.on('SIGINT', () => {
    console.log(`\n\x1b[31m[WATCHDOG] Shutting down...\x1b[0m`);
    isShuttingDown = true;
    if (daemonProcess && daemonProcess.pid) {
        if (process.platform === 'win32') {
            try {
                require('child_process').execSync(`taskkill /pid ${daemonProcess.pid} /t /f`);
            } catch(e) {}
        } else {
            daemonProcess.kill('SIGINT');
        }
    }
    process.exit(0);
});

console.log(`\x1b[32m=== LIVA IMMUTABLE WATCHDOG ONLINE ===\x1b[0m`);
startDaemon();
