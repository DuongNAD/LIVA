const fs = require('fs');
if (fs.existsSync('coverage/lcov.info')) {
    let content = fs.readFileSync('coverage/lcov.info', 'utf8');
    content = content.replace(/^SF:/gm, 'SF:openclaw-gateway/');
    fs.writeFileSync('coverage/lcov.info', content);
    console.log('Fixed lcov.info paths');
} else {
    console.error('coverage/lcov.info not found');
}
