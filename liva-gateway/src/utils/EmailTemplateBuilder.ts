/**
 * EmailTemplateBuilder — Threaded HTML Email Formatter
 * ===================================================
 * Safely converts Markdown responses from LLMs into standard HTML blocks
 * and wraps original quoted messages inside blockquotes formatted for Outlook, Gmail, etc.
 *
 * [v5.0] LIVA Remote Control Hub
 */

export class EmailTemplateBuilder {
    /**
     * Converts a simple markdown text into structured, clean HTML.
     */
    public static markdownToHtml(markdown: string): string {
        if (!markdown) return "";
        
        // Escape HTML tags to prevent cross-site scripting/injection
        let html = markdown
            .replaceAll("&", "&amp;")
            .replaceAll("<", "&lt;")
            .replaceAll(">", "&gt;");
        
        // Convert headers: ### header -> <h3>header</h3>
        html = html.replace(/^### (.*?)$/gm, "<h3>$1</h3>");
        html = html.replace(/^## (.*?)$/gm, "<h2>$1</h2>");
        html = html.replace(/^# (.*?)$/gm, "<h1>$1</h1>");
        
        // Bold: **text** -> <strong>text</strong>
        html = html.replace(/\*\*(.*?)\*\*/g, "<strong>$1</strong>");
        
        // Bullet lists: - item or * item -> <li>item</li>
        html = html.replace(/^[-\*] (.*?)$/gm, "<li>$1</li>");
        
        // Wrap consecutive <li> tags in <ul>
        html = html.replace(/((?:<li>.*?<\/li>\s*)+)/g, "<ul>\n$1</ul>");
        
        // Convert line breaks and wrap paragraphs
        const sections = html.split(/\n\n+/);
        html = sections
            .map(section => {
                const trimmed = section.trim();
                if (!trimmed) return "";
                if (
                    trimmed.startsWith("<h1>") || 
                    trimmed.startsWith("<h2>") || 
                    trimmed.startsWith("<h3>") || 
                    trimmed.startsWith("<ul>") || 
                    trimmed.startsWith("<li>")
                ) {
                    return trimmed;
                }
                return `<p>${trimmed.replaceAll("\n", "<br>")}</p>`;
            })
            .filter(Boolean)
            .join("\n");
            
        return html;
    }

    /**
     * Builds an email body with HTML reply and quotes the original email in a blockquote.
     */
    public static buildReplyHtml(
        replyMarkdown: string,
        originalSenderText: string,
        originalDateText: string,
        originalHtmlOrText: string,
        isHtml: boolean = false
    ): string {
        const replyHtml = this.markdownToHtml(replyMarkdown);
        let quotedContentHtml = "";
        
        if (isHtml) {
            quotedContentHtml = originalHtmlOrText;
        } else {
            // Escape original plain text and convert newlines to br
            const escapedText = originalHtmlOrText
                .replaceAll("&", "&amp;")
                .replaceAll("<", "&lt;")
                .replaceAll(">", "&gt;");
            quotedContentHtml = escapedText.split("\n").join("<br>");
        }

        const date = originalDateText || new Date().toUTCString();
        const sender = originalSenderText || "đối tác";

        const template = `
<!DOCTYPE html>
<html>
<head>
  <meta charset="utf-8">
  <style>
    body { 
      font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial, sans-serif; 
      font-size: 14px; 
      line-height: 1.5; 
      color: #333333; 
    }
    blockquote { 
      margin: 15px 0 0 0; 
      padding-left: 15px; 
      border-left: 2px solid #cccccc; 
      color: #666666; 
    }
    .original-header { 
      font-size: 13px; 
      color: #777777; 
      margin-bottom: 8px; 
      margin-top: 20px;
    }
  </style>
</head>
<body>
  <div class="reply-content">
    ${replyHtml}
  </div>
  <hr style="border:none; border-top:1px solid #e0e0e0; margin:20px 0 10px 0;" />
  <div class="original-header">
    On ${date}, ${sender} wrote:
  </div>
  <blockquote class="gmail_quote">
    ${quotedContentHtml}
  </blockquote>
</body>
</html>
`;
        return template.trim();
    }
}
