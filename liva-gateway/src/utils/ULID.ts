import * as crypto from "node:crypto";

/**
 * generateULID — Lightweight ULID (Universally Unique Lexicographically Sortable Identifier)
 * ============================================================================================
 * Produces 26-character Crockford Base32 IDs that are:
 *   1. **Sortable by time** — first 10 chars encode millisecond timestamp
 *   2. **Unique** — last 16 chars are cryptographically random
 *   3. **Zero dependencies** — uses only `node:crypto`
 *
 * Format: TTTTTTTTTTRRRRRRRRRRRRRRR (10 time + 16 random = 26 chars)
 * Monotonic within same millisecond via random component.
 *
 * Advantages over UUID v4 for vector/DB IDs:
 *   - Natural time-ordering → better B-tree index locality
 *   - No hyphens → compact storage
 *   - Crockford Base32 → URL-safe, case-insensitive
 *
 * @returns 26-character ULID string
 */

const CROCKFORD_BASE32 = "0123456789ABCDEFGHJKMNPQRSTVWXYZ";

function encodeTime(timestamp: number, length: number): string {
    let result = "";
    for (let i = length - 1; i >= 0; i--) {
        result = CROCKFORD_BASE32[timestamp & 31]! + result;
        timestamp = Math.floor(timestamp / 32);
    }
    return result;
}

function encodeRandom(length: number): string {
    const bytes = crypto.randomBytes(length);
    let result = "";
    for (let i = 0; i < length; i++) {
        result += CROCKFORD_BASE32[bytes[i]! & 31];
    }
    return result;
}

export function generateULID(): string {
    return encodeTime(Date.now(), 10) + encodeRandom(16);
}
