const fs = require('fs');
const path = require('path');

const targetFile = path.join(__dirname, 'src/core/AgentLoop.ts');
const content = fs.readFileSync(targetFile, 'utf8');
const lines = content.split('\n');

const before = lines.slice(0, 13);
const after = lines.slice(312);

const imports = [
    'export * from "../types/AgentTypes";',
    'import { AgentPhase, AuthorityToken, TaskLane, MessageTask, TaskState } from "../types/AgentTypes";',
    'import { CoreKernelAuthority } from "./CoreKernelAuthority";',
    'import { DualPortController } from "./DualPortController";',
    'import { ToolExecutionOrchestrator } from "./ToolExecutionOrchestrator";',
    'import { LTCOrchestrator } from "./LTCOrchestrator";',
    'import { TaskLaneWorker } from "./TaskLaneWorker";'
];

fs.writeFileSync(targetFile, [...before, ...imports, ...after].join('\n'), 'utf8');
console.log('Done!');
