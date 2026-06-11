import { parentPort } from "node:worker_threads";
import { createHash } from "node:crypto";
import { generateULID } from "../utils/ULID.js";

export interface MemoryNode {
    id: string;
    hash: string;
    content: string;
    weight: number;
    lastAccessed: number;
}

parentPort?.on("message", (msg: { rawLogs: string; existingNodes: MemoryNode[] }) => {
    try {
        const { rawLogs, existingNodes } = msg;
        const lines = rawLogs.split("\n").filter((l) => l.trim() !== "");
        
        const nodeMap = new Map<string, MemoryNode>();
        for (const node of existingNodes) {
            nodeMap.set(node.hash, { ...node });
        }

        // Process and deduplicate logs
        for (const line of lines) {
            try {
                const log = JSON.parse(line);
                const content = log.content;
                const timestamp = log.timestamp || Date.now();
                if (!content || !content.trim()) {
                    continue;
                }

                const hash = createHash("sha256").update(content.trim()).digest("hex");
                const existing = nodeMap.get(hash);
                if (existing) {
                    existing.weight += 1;
                    existing.lastAccessed = Math.max(existing.lastAccessed, timestamp);
                } else {
                    nodeMap.set(hash, {
                        id: generateULID(),
                        hash,
                        content: content.trim(),
                        weight: 1,
                        lastAccessed: timestamp,
                    });
                }
            } catch {
                // Ignore parsing errors for individual lines inside the worker
            }
        }

        // Construct proposed index and sort by weight descending
        const proposedIndex = Array.from(nodeMap.values()).sort((a, b) => b.weight - a.weight);

        // Generate Git-Diff styled report of proposed vs old memory nodes
        const oldMap = new Map(existingNodes.map((n) => [n.hash, n]));
        const newMap = new Map(proposedIndex.map((n) => [n.hash, n]));

        const diffLines: string[] = [];

        // Added nodes
        for (const [hash, node] of newMap.entries()) {
            if (!oldMap.has(hash)) {
                diffLines.push(`+ [ADDED] ID: ${node.id} (Weight: ${node.weight})`);
                diffLines.push(`+ ${node.content}`);
                diffLines.push("");
            }
        }

        // Modified nodes
        for (const [hash, node] of newMap.entries()) {
            const oldNode = oldMap.get(hash);
            if (oldNode && oldNode.weight !== node.weight) {
                diffLines.push(`~ [MODIFIED] ID: ${node.id}`);
                diffLines.push(`- Weight: ${oldNode.weight}`);
                diffLines.push(`+ Weight: ${node.weight}`);
                diffLines.push(`  Content: ${node.content}`);
                diffLines.push("");
            }
        }

        // Removed nodes
        for (const [hash, node] of oldMap.entries()) {
            if (!newMap.has(hash)) {
                diffLines.push(`- [REMOVED] ID: ${node.id} (Weight: ${node.weight})`);
                diffLines.push(`- ${node.content}`);
                diffLines.push("");
            }
        }

        parentPort?.postMessage({
            ok: true,
            proposedIndex,
            diffPayload: diffLines.join("\n").trim()
        });
    } catch (err: unknown) {
        parentPort?.postMessage({
            ok: false,
            error: err instanceof Error ? err.message : String(err)
        });
    }
});
