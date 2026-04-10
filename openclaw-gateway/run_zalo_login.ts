import { execute } from "./src/skills/SendZaloRPA.js";

async function main() {
  console.log("Preparing to send Zalo message to Mom...");
  try {
    const result = await execute({
      targetName: "Việt Nam",
      message: "m ổn k"
    });
    console.log("Result string from execute():", result);
  } catch (e) {
    console.error("Caught Exception:", e);
  }
}

main();
