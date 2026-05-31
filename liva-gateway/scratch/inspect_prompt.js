import { MemoryManager } from '../src/MemoryManager.js';
import dotenv from 'dotenv';
import path from 'path';

dotenv.config();

async function inspect() {
    console.log('=== PROMPT INSPECTOR ===');
    const memory = new MemoryManager("liv_async_core");
    await memory.initialize();

    console.log('\n--- Checking Memory Sizes ---');
    
    const userProfile = await memory.getUserProfile();
    console.log(`User Profile size: ${JSON.stringify(userProfile).length} chars`);

    const structuredPrompt = memory.getStructuredMemoryPrompt();
    console.log(`Structured Memory Prompt size: ${structuredPrompt.length} chars`);
    
    const ltcContent = await memory.getLongTermMarkdown();
    console.log(`Long Term Memory size: ${ltcContent?.length || 0} chars`);

    const sessionState = await memory.getSessionState();
    console.log(`Session State size: ${sessionState?.length || 0} chars`);

    const prevSessionContext = await memory.getPreviousSessionContextPrompt();
    console.log(`Prev Session Context size: ${prevSessionContext?.length || 0} chars`);

    const shortTermHistory = await memory.getHybridContext('test', 6);
    console.log(`Short Term History size: ${JSON.stringify(shortTermHistory).length} chars`);
    console.log(`Short Term History turns count: ${shortTermHistory.length}`);
    
    process.exit(0);
}

inspect().catch(console.error);
