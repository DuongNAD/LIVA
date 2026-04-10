import { execute } from "./src/skills/SendMessengerRPA.js";

async function main() {
  console.log("Preparing to start FB Messenger Web for Login...");
  try {
    const result = await execute({
      targetName: "Đức Khánh", 
      message: "ôn xong bài chưa"
    });
    console.log("Result string from execute():", result);
  } catch (e) {
    console.error("Caught Exception:", e);
  }
}

main();
