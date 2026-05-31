import * as dotenv from "dotenv";
import * as fs from "node:fs";
import * as path from "node:path";

// [STDOUT_GUARD] - Bảo vệ IPC Handshake khỏi rác Console
// eslint-disable-next-line no-console
console.log = (...args: unknown[]) => console.error('[STDOUT_GUARD]', ...args);
// eslint-disable-next-line no-console
console.warn = (...args: unknown[]) => console.error('[STDOUT_GUARD_WARN]', ...args);
// eslint-disable-next-line no-console
console.info = (...args: unknown[]) => console.error('[STDOUT_GUARD_INFO]', ...args);

// Dynamically resolve package directory to support launching from workspace root
const getGatewayDirectory = (): string => {
    const cwd = process.cwd();
    if (fs.existsSync(path.join(cwd, "liva-gateway"))) {
        return path.join(cwd, "liva-gateway");
    }
    return cwd;
};

const gatewayDir = getGatewayDirectory();
const envPath = path.join(gatewayDir, ".env");

dotenv.config({ path: envPath });
