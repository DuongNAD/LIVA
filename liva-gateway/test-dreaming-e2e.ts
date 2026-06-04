import { MemoryDreamingPipeline } from './src/memory/MemoryDreamingPipeline.js';
import * as path from 'path';

async function run() {
    console.log('Starting E2E test...');
    const storagePath = path.join(process.cwd(), 'data', 'agents', 'e2e-agent', 'memory_store');
    const pipeline = new MemoryDreamingPipeline(storagePath, 'e2e-agent');
    
    // Simulate some session logs
    await pipeline.appendSessionLog({
        sessionId: 'session_1',
        timestamp: Date.now(),
        content: 'This is a test session where the user talked about programming.',
        turnCount: 5,
        summary: 'User likes programming.',
        emotionalShift: 'neutral',
        newInsights: ['likes programming']
    });

    await pipeline.appendSessionLog({
        sessionId: 'session_2',
        timestamp: Date.now() + 1000,
        content: 'This is a test session where the user talked about programming.', // duplicate!
        turnCount: 5,
        summary: 'User likes programming.',
        emotionalShift: 'neutral',
        newInsights: ['likes programming']
    });
    
    await pipeline.appendSessionLog({
        sessionId: 'session_3',
        timestamp: Date.now() + 2000,
        content: 'User played some games today.',
        turnCount: 3,
        summary: 'User likes gaming.',
        emotionalShift: 'happy',
        newInsights: ['likes gaming']
    });

    const result = await pipeline.executeDreamingSequence();
    console.log('Dreaming result:', result);
    if (result) {
        console.log('Proposed index length:', result.proposedIndex.items.length);
        console.log('Compression ratio:', result.compressionRatio);
        if (result.compressionRatio > 0.3) {
            console.log('Auto-committing...');
            await pipeline.commitApprovedMemory(result.proposedIndex);
            console.log('Committed!');
        }
    }
}
run().catch(console.error);
