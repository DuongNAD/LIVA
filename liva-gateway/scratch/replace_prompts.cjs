const fs = require('fs');

const replacements = [
    {
        file: 'e:/Project/LIVA/liva-gateway/src/skills/agentic/ResearchIdeation.ts',
        targets: [
            ['description: "Chủ đề vĩ mô cần xin ý tưởng. Ví dụ: \\\'Kết hợp AI và Web\\\'."', 'description: "Macro topic for ideation. Example: \\\'Combining AI and Web\\\'."'],
            ['description: "Thư mục lưu Đề Xuất Nghiên cứu gốc. Khuyến nghị: E:/Project/LIVA/scratch_workspace"', 'description: "Directory to save original Research Proposal. Recommended: E:/Project/LIVA/scratch_workspace"']
        ]
    },
    {
        file: 'e:/Project/LIVA/liva-gateway/src/skills/core/SearchLocalFile.ts',
        targets: [
            ['description: "Từ khóa tên file cần tìm (ví dụ: \\\'CV\\\', \\\'baocao.pdf\\\'). Không cần gõ chính xác 100%."', 'description: "Filename keyword to search (e.g., \\\'CV\\\', \\\'report.pdf\\\'). Does not need to be 100% exact."']
        ]
    },
    {
        file: 'e:/Project/LIVA/liva-gateway/src/skills/core/UpdateTask.ts',
        targets: [
            ['description: "[AUTO_RUN] Cập nhật thông tin chi tiết (description), tiêu đề (title), hoặc trạng thái (status) của một kế hoạch/task trên hệ thống Dashboard của người dùng. Hãy dùng skill này để lưu lại lịch trình sau khi đã thảo luận xong với người dùng."', 'description: "[AUTO_RUN] Update task details (description, title, or status) on the user\\\'s Dashboard. Use this skill to save schedules/plans after discussing with the user."'],
            ['description: "Mã ID của task cần cập nhật (ví dụ: task_123456_abcdef)."', 'description: "Task ID to update (e.g., task_123456_abcdef)."'],
            ['description: "Tiêu đề mới của task (tuỳ chọn)."', 'description: "New task title (optional)."'],
            ['description: "Nội dung/lịch trình chi tiết đã được tóm tắt (tuỳ chọn)."', 'description: "Summarized details/schedule content (optional)."'],
            ['description: "Trạng thái mới (ví dụ: pending, in-progress, completed) (tuỳ chọn)."', 'description: "New task status (e.g., pending, in-progress, completed) (optional)."']
        ]
    },
    {
        file: 'e:/Project/LIVA/liva-gateway/src/skills/docs/PlanWriter.ts',
        targets: [
            ['description: "[VIETNAMESE] Project/plan name exactly as requested by user. Example: \\\'Kế hoạch ra mắt sản phẩm mới\\\', \\\'Lộ trình Marketing Q2\\\'."', 'description: "[LOCALIZED] Project/plan name exactly as requested by user. Example: \\\'New product launch plan\\\', \\\'Q2 Marketing Roadmap\\\'."']
        ]
    },
    {
        file: 'e:/Project/LIVA/liva-gateway/src/skills/docs/ReportWriter.ts',
        targets: [
            ['description: "[VIETNAMESE] Report topic exactly as user requested. Example: \\\'Báo cáo doanh thu tháng 4\\\', \\\'Báo cáo xu hướng AI 2024\\\'."', 'description: "[LOCALIZED] Report topic exactly as user requested. Example: \\\'April revenue report\\\', \\\'2024 AI trend report\\\'."']
        ]
    },
    {
        file: 'e:/Project/LIVA/liva-gateway/src/skills/personal/MediaController.ts',
        targets: [
            ['description: "[AUTO_RUN] Điều khiển đa phương tiện trên máy tính (Spotify, Youtube, Volume...). Hỗ trợ: Play/Pause, Next, Prev, Mute, Volume Up, Volume Down."', 'description: "[AUTO_RUN] Control PC media (Spotify, Youtube, Volume...). Supports: Play/Pause, Next, Prev, Mute, Volume Up, Volume Down."']
        ]
    },
    {
        file: 'e:/Project/LIVA/liva-gateway/src/skills/web/CryptoTracker.ts',
        targets: [
            ['description: "Danh sách các mã coin (symbol) cần tra cứu (ví dụ: [\\\'btc\\\', \\\'eth\\\', \\\'sol\\\']). Nếu người dùng hỏi chung chung, hãy tự động liệt kê [\\\'btc\\\', \\\'eth\\\', \\\'bnb\\\', \\\'sol\\\']."', 'description: "List of coin symbols to lookup (e.g., [\\\'btc\\\', \\\'eth\\\', \\\'sol\\\']). If user asks generally, default to [\\\'btc\\\', \\\'eth\\\', \\\'bnb\\\', \\\'sol\\\']."']
        ]
    },
    {
        file: 'e:/Project/LIVA/liva-gateway/src/skills/web/LocationSearch.ts',
        targets: [
            ['description: "[LOCALIZED] The specific location name, street, or address to search (e.g., \\\'Landmark 81\\\', \\\'quán cafe gần đây\\\'). Provide the query in the user\\\'s language."', 'description: "[LOCALIZED] Specific location name, street, or address to search (e.g., \\\'Landmark 81\\\', \\\'nearby cafe\\\'). Provide query in user\\\'s language."']
        ]
    },
    {
        file: 'e:/Project/LIVA/liva-gateway/src/core/bootstrap/BootstrapManager.ts',
        targets: [
            ['await this.#deps.dispatch("agent_input", `[System Cognitive Event]: Người dùng vừa cài đặt ứng dụng \\\'${appName}\\\' lên máy tính. Bạn vừa được nạp kỹ năng điều khiển \\\'${skillData.type}\\\' (${skillData.description}). Hãy RẤT HÀO HỨNG khoe với người dùng rằng bạn đã biết họ cài app mới và đề xuất một hành động ngay lập tức! (Không cần xưng hô System)`);', 'await this.#deps.dispatch("agent_input", `[System Cognitive Event]: User just installed the app \\\'${appName}\\\'. You have been granted the \\\'${skillData.type}\\\' skill (${skillData.description}). Be VERY EXCITED to tell the user you noticed the new app and proactively suggest an immediate action! (Do not mention \\\'System\\\')`);']
        ]
    },
    {
        file: 'e:/Project/LIVA/liva-gateway/src/core/CoreKernel.ts',
        targets: [
            ['await this.#dispatch("agent_input", `[System Cognitive Event]: Người dùng vừa cài đặt ứng dụng \\\'${appName}\\\' lên máy tính. Bạn vừa được nạp kỹ năng điều khiển \\\'${skillData.type}\\\' (${skillData.description}). Hãy RẤT HÀO HỨNG khoe với người dùng rằng bạn đã biết họ cài app mới và đề xuất một hành động ngay lập tức! (Không cần xưng hô System)`);', 'await this.#dispatch("agent_input", `[System Cognitive Event]: User just installed the app \\\'${appName}\\\'. You have been granted the \\\'${skillData.type}\\\' skill (${skillData.description}). Be VERY EXCITED to tell the user you noticed the new app and proactively suggest an immediate action! (Do not mention \\\'System\\\')`);'],
            ['${task.description ? `Mô tả ban đầu: ${task.description}` : ""}', '${task.description ? `Initial description: ${task.description}` : ""}']
        ]
    },
    {
        file: 'e:/Project/LIVA/liva-gateway/src/services/AppWatcherService.ts',
        targets: [
            ['const eventContext = `[System Event]: Hệ điều hành phát hiện người dùng vừa cài đặt ứng dụng \\\'${appName}\\\'. Bạn hiện đã được cấp quyền truy cập công cụ \\\'${skillData.type}\\\' (${skillData.description || "Điều khiển ứng dụng"}). Hãy RẤT HÀO HỨNG thông báo điều này cho người dùng và tự động đề xuất 1 hành động liên quan đến ứng dụng này ngay lập tức.`;', 'const eventContext = `[System Event]: OS detected user just installed \\\'${appName}\\\'. You are now granted access to tool \\\'${skillData.type}\\\' (${skillData.description || "App Controller"}). Be VERY EXCITED to inform the user and proactively suggest an action related to this app immediately.`;']
        ]
    }
];

let successCount = 0;
for (const entry of replacements) {
    if (fs.existsSync(entry.file)) {
        let content = fs.readFileSync(entry.file, 'utf8');
        let modified = false;
        for (const [target, replacement] of entry.targets) {
            if (content.includes(target)) {
                content = content.replace(target, replacement);
                modified = true;
            } else {
                console.warn(`Target not found in ${entry.file}: \n` + target);
            }
        }
        if (modified) {
            fs.writeFileSync(entry.file, content, 'utf8');
            console.log(`Updated ${entry.file}`);
            successCount++;
        }
    } else {
        console.warn(`File not found: ${entry.file}`);
    }
}
console.log(`Successfully updated ${successCount} files.`);
