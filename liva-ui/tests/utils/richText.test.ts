import { describe, expect, it } from "vitest";
import { renderSafeRichText } from "../../src/utils/richText";

describe("renderSafeRichText", () => {
  it("escapes model-provided HTML and event handlers", () => {
    const rendered = renderSafeRichText(
      `<img src=x onerror="window.pwned=true"><script>alert(1)</script>`,
    );

    expect(rendered).not.toContain("<img");
    expect(rendered).not.toContain("<script");
    expect(rendered).not.toContain("onclick=");
    expect(rendered).toContain("&lt;img");
    expect(rendered).toContain("&lt;script&gt;");
  });

  it("preserves line breaks using generated markup only", () => {
    expect(renderSafeRichText("line 1\nline 2")).toBe("line 1<br/>line 2");
    expect(renderSafeRichText("line 1<br>line 2")).toBe("line 1<br/>line 2");
  });

  it("generates only whitelisted channel buttons without inline JavaScript", () => {
    const rendered = renderSafeRichText(
      "- 💬 Zalo\n- 📘 Messenger\n- 📧 Email\n<img src=x onerror=alert(1)>",
    );

    expect(rendered).toContain('data-liva-channel="Zalo"');
    expect(rendered).toContain('data-liva-channel="Messenger"');
    expect(rendered).toContain('data-liva-channel="Email"');
    expect(rendered).not.toContain("onclick=");
    expect(rendered).not.toContain("<img");
  });
});
