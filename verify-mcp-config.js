const fs = require('fs');
const path = require('path');

const configPath = 'C:\\Users\\Admin\\.gemini\\antigravity\\mcp_config.json';
console.log(`Checking config at: ${configPath}`);

if (!fs.existsSync(configPath)) {
  console.error(`Error: config file does not exist.`);
  process.exit(1);
}

try {
  const content = fs.readFileSync(configPath, 'utf8');
  const config = JSON.parse(content);
  
  if (!config.mcpServers) {
    throw new Error("Missing 'mcpServers' key.");
  }
  
  const obsidian = config.mcpServers.obsidian;
  if (!obsidian) {
    throw new Error("Missing 'obsidian' server in mcpServers.");
  }
  
  if (obsidian.command !== 'node') {
    throw new Error(`Expected command to be 'node', got '${obsidian.command}'`);
  }
  
  if (!Array.isArray(obsidian.args)) {
    throw new Error("Expected 'args' to be an array.");
  }
  
  if (obsidian.args.length !== 2) {
    throw new Error(`Expected 'args' length to be 2, got ${obsidian.args.length}`);
  }
  
  const expectedArg1 = "E:\\Project\\LIVA\\teamwork_projects\\obsidian_llm_wiki\\dist\\src\\index.js";
  const expectedArg2 = "E:\\Project\\LIVA\\teamwork_projects\\obsidian_llm_wiki\\vault";
  
  if (obsidian.args[0] !== expectedArg1) {
    throw new Error(`Expected first arg to be '${expectedArg1}', got '${obsidian.args[0]}'`);
  }
  
  if (obsidian.args[1] !== expectedArg2) {
    throw new Error(`Expected second arg to be '${expectedArg2}', got '${obsidian.args[1]}'`);
  }
  
  console.log("🎉 Success: obsidian MCP server configuration is verified successfully!");
  process.exit(0);
} catch (err) {
  console.error(`Verification Failed: ${err.message}`);
  process.exit(1);
}
