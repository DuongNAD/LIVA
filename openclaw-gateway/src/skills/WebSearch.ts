export const metadata = {
  name: "web_search",
  search_keywords: ["web_search","web search","tìm kiếm","tra cứu"],
  description:
    "Tìm kiếm thông tin trên Internet (Web Search). Sử dụng khi cần tra cứu các kiến thức, định nghĩa, hoặc thông tin mà AI chưa biết chắc chắn.",
  parameters: {
    type: "object",
    properties: {
      query: {
        type: "string",
        description: "Từ khóa cần tìm kiếm (Search query).",
      },
    },
    required: ["query"],
  },
};

export const execute = async (args: { query: string }): Promise<string> => {
  try {
    console.log(
      `[Skill: web_search] Đang tìm kiếm (Searching) từ khóa: "${args.query}"`,
    );

    // Sử dụng DuckDuckGo HTML Search để lấy link web đời thực
    const url = `https://html.duckduckgo.com/html/?q=${encodeURIComponent(args.query)}`;
    const response = await fetch(url, {
      headers: {
        "User-Agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64)",
      },
    });

    if (!response.ok) {
      throw new Error(
        `Lỗi kết nối Web Search (HTTP error): ${response.status}`,
      );
    }

    const html = await response.text();

    // Cào dữ liệu Link, Title bằng Regex vì chạy Nodejs không có DOM
    const results: { title: string; link: string }[] = [];
    const resultRegex = /<a class="result__url" href="([^"]+)">([^<]+)<\/a>/g;

    let match;
    while ((match = resultRegex.exec(html)) !== null && results.length < 10) {
      // Duckduckgo có chứa đoạn redirect qua tham số, xử lý làm mượt link URL nếu cần
      let link = match[1];
      if (link.startsWith("//duckduckgo.com/l/?uddg=")) {
        try {
          link = decodeURIComponent(link.split("uddg=")[1].split("&")[0]);
        } catch (e) {}
      }
      results.push({
        link: link,
        title: match[2].trim(),
      });
    }

    if (results.length === 0) {
      return `Không tìm thấy kết quả nào (No results found) trên web cho "${args.query}".`;
    }

    let output = `[Web Search] Top ${results.length} bài viết cho "${args.query}":\n`;
    results.forEach((r, i) => {
      output += `${i + 1}. ${r.title}\n   Link: ${r.link}\n`;
    });

    return output;
  } catch (error: any) {
    return `Lỗi tìm kiếm (Search error): ${error.message}`;
  }
};
