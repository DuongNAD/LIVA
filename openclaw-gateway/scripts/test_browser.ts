import { execute } from "./skills/web/WebBrowser";

async function test() {
  console.log("=== THỰC THI ACTION: NAVEGATE ===");
  console.log(
    await execute({ action: "navigate", url: "https://example.com" }),
  );

  console.log("\n=== THỰC THI ACTION: EXTRACT ===");
  console.log(await execute({ action: "extract", selector: "h1" }));

  console.log("\n=== THỰC THI ACTION: CLOSE ===");
  console.log(await execute({ action: "close" }));
}
test();
