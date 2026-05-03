import * as fs from 'fs/promises';
import * as path from 'path';

const SKILLS_DIR = path.join(process.cwd(), 'src', 'skills');

const DOMAINS = {
    agentic: ['AIScientist.ts', 'ResearchIdeation.ts'],
    web: ['WebSearch.ts', 'WebBrowser.ts', 'GeminiSurfer.ts', 'ComputerUse.ts'],
    personal: [
        'ObsidianOperator.ts', 'DesktopRPA.ts', 'HardwareController.ts', 'SystemHealth.ts',
        'WorkspaceManager.ts', 'MediaController.ts', 'ClipboardManager.ts', 'VoiceSpeaker.ts',
        'WindowArranger.ts', 'AppLauncher.ts', 'NotificationPusher.ts', 'TimerReminder.ts',
        'CalendarScheduler.ts'
    ],
    data: [
        'DBOperator.ts', 'ZipOperator.ts', 'FileOrganizer.ts', 'ClipboardOCR.ts',
        'ChartGenerator.ts', 'VisionAnalyzer.ts', 'StructuredDataAnalyzer.ts'
    ],
    social: [
        'SocialMediaPoster.ts', 'SendMessengerRPA.ts', 'SendZaloRPA.ts', 'SendZaloBot.ts',
        'SendEmail.ts', 'ReadEmails.ts', 'CheckImportantEmailsToday.ts', 'ReadRecentEmails.ts'
    ],
    devops: [
        'DockerSandboxManager.ts', 'ExecuteCommand.ts', 'GitOperator.ts', 'GitNexusQuery.ts',
        'GitSyncProject.ts', 'LogAnalyzer.ts', 'GetSystemInfo.ts'
    ],
    docs: [
        'CreateGoogleDoc.ts', 'AppendGoogleDoc.ts', 'ReadGoogleSheet.ts', 'WriteGoogleSheet.ts',
        'DocumentParser.ts', 'DocumentWriterBase.ts', 'ReportWriter.ts', 'PlanWriter.ts',
        'SearchGoogleDrive.ts'
    ],
    core: [
        'UpdateSessionState.ts', 'UpdateMemory.ts', 'UpdateCoreProfile.ts', 'LinearJiraTracker.ts',
        'ReadLocalFile.ts', 'WriteLocalFile.ts', 'DeleteLocalFile.ts', 'ListDirectory.ts',
        'OpenLocalFile.ts'
    ]
};

async function refactor() {
    console.log("🚀 Bắt đầu Refactor Architecture...");
    
    // 1. Rename update_session_state.ts to UpdateSessionState.ts if it exists
    try {
        await fs.rename(
            path.join(SKILLS_DIR, 'update_session_state.ts'),
            path.join(SKILLS_DIR, 'UpdateSessionState.ts')
        );
        console.log("Renamed update_session_state.ts -> UpdateSessionState.ts");
    } catch (e) {
        // Ignore if already renamed
    }

    // 2. Create folders
    for (const domain of Object.keys(DOMAINS)) {
        await fs.mkdir(path.join(SKILLS_DIR, domain), { recursive: true });
    }

    // 3. Move files and replace imports
    const files = await fs.readdir(SKILLS_DIR);
    const tsFiles = files.filter(f => f.endsWith('.ts') && f !== 'index.ts' && !Object.keys(DOMAINS).includes(f));
    
    // Fallback: If a file isn't in DOMAINS mapped exactly, put it in 'core'
    let fileDomainMap = {};
    for (const file of tsFiles) {
        let targetDomain = 'core';
        for (const [domain, list] of Object.entries(DOMAINS)) {
            if (list.includes(file)) {
                targetDomain = domain;
                break;
            }
        }
        fileDomainMap[file] = targetDomain;
    }

    for (const [file, domain] of Object.entries(fileDomainMap)) {
        const oldPath = path.join(SKILLS_DIR, file);
        const newPath = path.join(SKILLS_DIR, domain, file);

        let content = await fs.readFile(oldPath, 'utf8');
        // Rewrite relative imports
        // ../utils/logger -> @utils/logger
        // ../core/something -> @core/something
        // ../memory/something -> @memory/something
        content = content.replace(/from\s+["']\.\.\/utils\/(.*?)["']/g, 'from "@utils/$1"');
        content = content.replace(/from\s+["']\.\.\/core\/(.*?)["']/g, 'from "@core/$1"');
        content = content.replace(/from\s+["']\.\.\/memory\/(.*?)["']/g, 'from "@memory/$1"');
        
        await fs.writeFile(newPath, content, 'utf8');
        await fs.unlink(oldPath);
        console.log(`Moved ${file} -> ${domain}/`);
    }

    // 4. Create index.ts for each domain
    for (const [domain, list] of Object.entries(DOMAINS)) {
        let indexContent = "";
        const actualFiles = await fs.readdir(path.join(SKILLS_DIR, domain));
        const moduleNames = actualFiles.filter(f => f.endsWith('.ts') && f !== 'index.ts').map(f => f.replace('.ts', ''));
        
        for (const mod of moduleNames) {
            indexContent += `export * as ${mod} from './${mod}';\n`;
        }
        await fs.writeFile(path.join(SKILLS_DIR, domain, 'index.ts'), indexContent, 'utf8');
        console.log(`Created index.ts for ${domain}`);
    }

    console.log("✅ Hoàn thành quy hoạch vật lý!");
}

refactor().catch(console.error);
