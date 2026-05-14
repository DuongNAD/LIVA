import * as dns from "node:dns/promises";
import * as os from "node:os";
import { safeFetch } from "@utils/HttpClient";
import { logger } from "@utils/logger";

import { SkillMetadata } from "../SkillMetadata";

export const metadata: SkillMetadata = {
  name: "network_diagnostics",
  category: "devops",
  short_desc: "Run diagnostics on IP/Domain or network.",
  semantic_tags: ["#network", "#ping", "#dns", "#speedtest", "#mang"],
  search_keywords: ["network", "mạng", "ping", "internet", "speed", "dns", "tốc độ", "kết nối", "wifi"],
  description: "[AUTO_RUN] Run network diagnostics: ping, DNS lookup, speed test, and full health check. Use when user asks about network/internet status.",
  parameters: {
    type: "object",
    properties: {
      action: {
        type: "string",
        enum: ["ping", "dns", "speed", "full"],
        description: "'ping' = check connectivity, 'dns' = resolve hostname, 'speed' = download speed test, 'full' = all checks. Default: 'full'.",
      },
      host: { type: "string", description: "Target hostname for ping/dns (default: 'google.com')." },
    },
    required: [],
  },
};

async function checkPing(host: string): Promise<string> {
  const start = performance.now();
  try {
    await safeFetch(`https://${host}`, { method: "HEAD" }, 5000);
    const latency = (performance.now() - start).toFixed(0);
    return `✅ ${host}: Reachable (${latency}ms)`;
  } catch {
    return `❌ ${host}: Unreachable`;
  }
}

async function checkDNS(host: string): Promise<string> {
  try {
    const addresses = await dns.resolve4(host);
    return `✅ DNS ${host} → ${addresses.join(", ")}`;
  } catch (err: unknown) {
    const errMsg = err instanceof Error ? err.message : String(err);
    return `❌ DNS resolution failed for ${host}: ${errMsg}`;
  }
}

async function checkSpeed(): Promise<string> {
  // Download a small file and measure speed
  const testUrl = "https://speed.cloudflare.com/__down?bytes=1000000"; // 1MB
  const start = performance.now();
  try {
    const response = await safeFetch(testUrl, {}, 15000);
    const buffer = await response.arrayBuffer();
    const elapsed = (performance.now() - start) / 1000; // seconds
    const sizeMB = buffer.byteLength / (1024 * 1024);
    const speedMbps = ((sizeMB * 8) / elapsed).toFixed(2);
    return `✅ Download speed: ~${speedMbps} Mbps (${sizeMB.toFixed(1)}MB in ${elapsed.toFixed(1)}s)`;
  } catch {
    return "❌ Speed test failed (could not reach test server)";
  }
}

function getNetworkInfo(): string {
  const interfaces = os.networkInterfaces();
  const results: string[] = [];

  for (const [name, addrs] of Object.entries(interfaces)) {
    if (!addrs) continue;
    for (const addr of addrs) {
      if (addr.family === "IPv4" && !addr.internal) {
        results.push(`🌐 ${name}: ${addr.address} (MAC: ${addr.mac})`);
      }
    }
  }
  return results.length > 0 ? results.join("\n") : "No external network interfaces found.";
}

export const execute = async (args: {
  action?: "ping" | "dns" | "speed" | "full";
  host?: string;
}): Promise<string> => {
  const action = args.action || "full";
  const host = args.host?.trim() || "google.com";
  const results: string[] = ["[NETWORK DIAGNOSTICS REPORT]\n"];

  logger.info(`[Skill: network_diagnostics] Running ${action} check on ${host}`);

  try {
    if (action === "ping" || action === "full") {
      results.push(await checkPing(host));
      if (action === "full") {
        results.push(await checkPing("cloudflare.com"));
        results.push(await checkPing("github.com"));
      }
    }

    if (action === "dns" || action === "full") {
      results.push(await checkDNS(host));
    }

    if (action === "speed" || action === "full") {
      results.push(await checkSpeed());
    }

    if (action === "full") {
      results.push("\n--- Local Network ---");
      results.push(getNetworkInfo());
    }

    return results.join("\n");
  } catch (error: unknown) {
    const errMsg = error instanceof Error ? error.message : String(error);
    return `Network diagnostics error: ${errMsg}`;
  }
};
