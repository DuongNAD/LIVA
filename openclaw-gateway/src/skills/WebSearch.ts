export const metadata = {
    name: "web_search",
    description: "Tìm kiếm thông tin trên Internet (Web Search). Sử dụng khi cần tra cứu các kiến thức, định nghĩa, hoặc thông tin mà AI chưa biết chắc chắn.",
    parameters: {
        type: "object",
        properties: {
            query: {
                type: "string",
                description: "Từ khóa cần tìm kiếm (Search query)."
            }
        },
        required: ["query"]
    }
};

export const execute = async (args: { query: string }): Promise<string> => {
    try {
        console.log(`[Skill: web_search] Đang tìm kiếm (Searching) từ khóa: "${args.query}"`);
        
        // Sử dụng API tìm kiếm mở của Wikipedia
        const url = `https://vi.wikipedia.org/w/api.php?action=opensearch&search=${encodeURIComponent(args.query)}&limit=3&format=json`;
        const response = await fetch(url);
        
        if (!response.ok) {
            throw new Error(`Lỗi mạng (HTTP error): ${response.status}`);
        }

        const data = await response.json();
        const titles = data[1]; // Mảng chứa các tiêu đề bài viết
        const links = data[3];  // Mảng chứa các đường dẫn tương ứng

        if (titles.length === 0) {
            return `Không tìm thấy kết quả nào (No results found) cho "${args.query}".`;
        }

        let result = `Kết quả tìm kiếm (Search results) cho "${args.query}":\n`;
        for (let i = 0; i < titles.length; i++) {
            result += `- ${titles[i]}: ${links[i]}\n`;
        }
        
        return result;
    } catch (error: any) {
        return `Lỗi tìm kiếm (Search error): ${error.message}`;
    }
};