// import { } from "./";

/**
 * AxTreeParser — Accessibility Tree Parser for Token-Efficient Browser Control
 * ==============================================================================
 * Converts Chrome's raw Accessibility Tree into a compact semantic table
 * that E4B can reason about using ~95% fewer tokens than raw DOM/HTML.
 * 
 * Key Insight (from browser-harness research):
 *   Raw HTML button: `<button class="btn-primary css-x7z9" aria-label="Submit">
 *                      <div><svg>...</svg><span>Submit</span></div></button>`
 *   → 50-100 tokens
 * 
 *   AxTree equivalent: `{role: "button", name: "Submit Form"}`
 *   → 5-10 tokens (95% reduction)
 * 
 * Architecture:
 *   1. CDPClient.getAccessibilityTree() → raw AX nodes from Chrome
 *   2. AxTreeParser.parse() → filters, prunes, assigns IDs
 *   3. AxTreeParser.formatSnapshot() → compact table string for LLM context
 * 
 * Performance:
 *   - Tree pruning: removes hidden, presentational, and non-interactive nodes
 *   - Token budget: auto-truncates if snapshot exceeds configured limit
 *   - No Worker Thread needed for typical pages (<5ms parse time)
 */

// ============================================================
// Types
// ============================================================

export interface AxElement {
    /** Unique identifier assigned by parser (used by E4B to target elements) */
    id: number;
    /** ARIA role: button, link, textbox, heading, img, etc. */
    role: string;
    /** Accessible name (text content or aria-label) */
    name: string;
    /** Current value (for inputs, selects, sliders) */
    value?: string;
    /** State flags: focused, disabled, checked, expanded, selected */
    state?: string[];
    /** Bounding box for click targeting (if available) */
    bounds?: { x: number; y: number; width: number; height: number };
    /** Nesting depth for structural context */
    depth: number;
}

/** Raw AX node from Chrome's Accessibility.getFullAXTree */
interface RawAxNode {
    nodeId: string;
    ignored?: boolean;
    role?: { type: string; value: string };
    name?: { type: string; value: string; sources?: any[] };
    description?: { type: string; value: string };
    value?: { type: string; value: any };
    properties?: Array<{ name: string; value: { type: string; value: any } }>;
    childIds?: string[];
    backendDOMNodeId?: number;
    parentId?: string;
}

// ============================================================
// Roles & Filters
// ============================================================

/** Roles that represent interactive elements (E4B can act on these) */
const INTERACTIVE_ROLES = new Set([
    "button", "link", "textbox", "checkbox", "radio",
    "combobox", "listbox", "menuitem", "tab", "switch",
    "slider", "spinbutton", "searchbox", "option",
    "menuitemcheckbox", "menuitemradio", "treeitem",
]);

/** Roles that provide structural/semantic context */
const SEMANTIC_ROLES = new Set([
    "heading", "img", "banner", "navigation", "main",
    "complementary", "contentinfo", "region", "article",
    "dialog", "alertdialog", "alert", "status", "timer",
    "table", "row", "cell", "columnheader", "rowheader",
    "list", "listitem", "form", "group",
]);

/** Roles to completely ignore (noise reduction) */
const IGNORED_ROLES = new Set([
    "none", "presentation", "generic", "InlineTextBox",
    "LineBreak", "StaticText", "paragraph", "div",
    "LayoutTable", "LayoutTableRow", "LayoutTableCell",
    "Abbr", "Ruby", "RubyAnnotation",
]);

/** ARIA states we care about */
const RELEVANT_STATES = new Set([
    "focused", "disabled", "checked", "expanded", "selected",
    "pressed", "required", "readonly", "invalid",
]);

// ============================================================
// Parser
// ============================================================

/**
 * Parse raw Accessibility Tree nodes from Chrome CDP into compact AxElements.
 * 
 * @param rawNodes - Array of raw AX nodes from Accessibility.getFullAXTree()
 * @param options - Parser configuration
 * @returns Array of parsed, filtered, and ID-assigned elements
 */
export function parseAxTree(
    rawNodes: RawAxNode[],
    options: {
        /** Include non-interactive semantic elements (headings, images) */
        includeSemanticNodes?: boolean;
        /** Maximum number of elements to return */
        maxElements?: number;
        /** Minimum name length to include (filters empty-name noise) */
        minNameLength?: number;
    } = {}
): AxElement[] {
    const {
        includeSemanticNodes = true,
        maxElements = 200,
        minNameLength = 1,
    } = options;

    const elements: AxElement[] = [];
    let nextId = 1;

    // Build parent depth map for tree pruning
    const depthMap = new Map<string, number>();
    const rootNodes = rawNodes.filter(n => !n.parentId);
    for (const root of rootNodes) {
        depthMap.set(root.nodeId, 0);
    }

    // BFS to calculate depths
    const queue = [...rootNodes];
    while (queue.length > 0) {
        const node = queue.shift()!;
        const parentDepth = depthMap.get(node.nodeId) ?? 0;

        if (node.childIds) {
            for (const childId of node.childIds) {
                depthMap.set(childId, parentDepth + 1);
                const child = rawNodes.find(n => n.nodeId === childId);
                if (child) queue.push(child);
            }
        }
    }

    for (const node of rawNodes) {
        // Skip ignored nodes
        if (node.ignored) continue;

        const role = node.role?.value ?? "";

        // Skip noise roles
        if (IGNORED_ROLES.has(role)) continue;

        // Determine if this node is worth including
        const isInteractive = INTERACTIVE_ROLES.has(role);
        const isSemantic = SEMANTIC_ROLES.has(role);

        if (!isInteractive && !(includeSemanticNodes && isSemantic)) continue;

        // Extract name
        const name = (node.name?.value ?? "").trim();

        // Skip elements with empty/too-short names (usually noise)
        if (name.length < minNameLength && !node.value) continue;

        // Extract value
        const value = node.value?.value;

        // Extract relevant states
        const states: string[] = [];
        if (node.properties) {
            for (const prop of node.properties) {
                if (RELEVANT_STATES.has(prop.name) && prop.value?.value === true) {
                    states.push(prop.name);
                }
            }
        }

        const element: AxElement = {
            id: nextId++,
            role,
            name: name.substring(0, 100), // Cap name length for token budget
            depth: depthMap.get(node.nodeId) ?? 0,
        };

        if (value !== undefined && value !== null && value !== "") {
            element.value = String(value).substring(0, 50);
        }

        if (states.length > 0) {
            element.state = states;
        }

        elements.push(element);

        // Enforce max elements cap
        if (elements.length >= maxElements) break;
    }

    return elements;
}

/**
 * Format parsed AxElements into a compact, LLM-friendly string.
 * 
 * Output format (each line ~5-15 tokens):
 * ```
 * id=1 | heading | "Google Search"
 * id=2 | textbox | "Search" [focused]
 * id=3 | button | "Google Search"
 * id=4 | button | "I'm Feeling Lucky"
 * id=5 | link | "Gmail"
 * ```
 * 
 * @param elements - Parsed AxElements
 * @param tokenBudget - Max approximate tokens (1 token ≈ 4 chars)
 */
export function formatAxSnapshot(
    elements: AxElement[],
    tokenBudget: number = 2000
): string {
    if (elements.length === 0) {
        return "[AxTree] Trang trống hoặc không có phần tử tương tác.";
    }

    const lines: string[] = [];
    let charCount = 0;
    const charBudget = tokenBudget * 4; // ~4 chars per token

    for (const el of elements) {
        let line = `id=${el.id} | ${el.role} | "${el.name}"`;

        if (el.value) {
            line += ` [value="${el.value}"]`;
        }

        if (el.state && el.state.length > 0) {
            line += ` [${el.state.join(", ")}]`;
        }

        // Check token budget
        if (charCount + line.length > charBudget) {
            lines.push(`... (cắt ngắn, còn ${elements.length - lines.length} phần tử nữa)`);
            break;
        }

        lines.push(line);
        charCount += line.length + 1; // +1 for newline
    }

    const header = `[AxTree Snapshot — ${elements.length} elements]`;
    return header + "\n" + lines.join("\n");
}

/**
 * Extract only interactive elements from the full AxTree.
 * Used when E4B needs to decide which element to click/type.
 */
export function getInteractiveElements(elements: AxElement[]): AxElement[] {
    return elements.filter(el => INTERACTIVE_ROLES.has(el.role));
}

/**
 * Find an element by role and partial name match.
 * Used for semantic element targeting (more robust than CSS selectors).
 */
export function findElement(
    elements: AxElement[],
    role: string,
    namePattern: string | RegExp
): AxElement | undefined {
    return elements.find(el => {
        if (el.role !== role) return false;
        if (typeof namePattern === "string") {
            return el.name.toLowerCase().includes(namePattern.toLowerCase());
        }
        return namePattern.test(el.name);
    });
}

/**
 * Get summary statistics about the AxTree (useful for debugging).
 */
export function getAxTreeStats(elements: AxElement[]): {
    total: number;
    interactive: number;
    semantic: number;
    estimatedTokens: number;
} {
    const interactive = elements.filter(el => INTERACTIVE_ROLES.has(el.role)).length;
    const semantic = elements.filter(el => SEMANTIC_ROLES.has(el.role)).length;
    const snapshot = formatAxSnapshot(elements);
    const estimatedTokens = Math.ceil(snapshot.length / 4);

    return { total: elements.length, interactive, semantic, estimatedTokens };
}
