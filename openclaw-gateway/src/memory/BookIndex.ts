import Graph from 'graphology';
import { bfsFromNode } from 'graphology-traversal';
import { logger } from '../utils/logger';

/**
 * BookIndex — Hierarchical Tree for RAPTOR Pipeline (Phase 2A)
 * ==========================================================
 * Manages a tree of document chunks and their summaries using Graphology.
 * Supports multi-hop reasoning by indexing documents at multiple levels of abstraction.
 */

export interface BookNode {
    id: string;
    text: string;
    level: number;
    isSummary: boolean;
    vector?: number[];
    clusterId?: string; // Identifier for clustering during RAPTOR summary
}

export class BookIndex {
    private graph: Graph;

    constructor() {
        this.graph = new Graph({ directed: true });
    }

    /**
     * Add a node to the hierarchical tree.
     */
    public addNode(node: BookNode): void {
        if (!this.graph.hasNode(node.id)) {
            this.graph.addNode(node.id, node);
            logger.debug(`[BookIndex] Added node ${node.id} at level ${node.level}`);
        } else {
            // Update node
            this.graph.replaceNodeAttributes(node.id, node);
        }
    }

    /**
     * Add a directed edge from a parent summary node to a child node.
     */
    public addEdge(parentId: string, childId: string): void {
        if (!this.graph.hasNode(parentId)) {
            logger.warn(`[BookIndex] Parent node ${parentId} not found.`);
            return;
        }
        if (!this.graph.hasNode(childId)) {
            logger.warn(`[BookIndex] Child node ${childId} not found.`);
            return;
        }
        
        if (!this.graph.hasEdge(parentId, childId)) {
            this.graph.addEdge(parentId, childId);
        }
    }

    /**
     * Get node attributes.
     */
    public getNode(id: string): BookNode | null {
        if (this.graph.hasNode(id)) {
            return this.graph.getNodeAttributes(id) as BookNode;
        }
        return null;
    }

    /**
     * Get all children of a node.
     */
    public getChildren(parentId: string): BookNode[] {
        if (!this.graph.hasNode(parentId)) return [];
        return this.graph.outNeighbors(parentId).map(id => this.getNode(id)!);
    }

    /**
     * Get all nodes at a specific hierarchical level.
     * Level 0 = Leaf nodes (raw chunks)
     * Level > 0 = Summaries
     */
    public getAllNodesAtLevel(level: number): BookNode[] {
        const nodes: BookNode[] = [];
        this.graph.forEachNode((node, attributes) => {
            if (attributes.level === level) {
                nodes.push(attributes as BookNode);
            }
        });
        return nodes;
    }

    /**
     * Traverse the tree using BFS starting from a given node.
     */
    public traverseBFS(startNodeId: string, callback: (node: BookNode) => void): void {
        if (!this.graph.hasNode(startNodeId)) return;
        
        bfsFromNode(this.graph, startNodeId, (node, attributes) => {
            callback(attributes as BookNode);
        });
    }

    /**
     * Get the total number of nodes in the index.
     */
    public get nodeCount(): number {
        return this.graph.order;
    }

    /**
     * Clear the index.
     */
    public clear(): void {
        this.graph.clear();
        logger.info("[BookIndex] Index cleared.");
    }
}
