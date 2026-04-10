import { execute } from './src/skills/WebSearch.js';
execute({ query: "Trí tuệ nhân tạo" }).then(res => {
    console.log(res);
    process.exit(0);
});
