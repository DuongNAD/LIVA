import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import test from 'node:test'

const readRepoFile = (path) => readFile(new URL(`../${path}`, import.meta.url), 'utf8')

test('widget chỉ trình bày một wake phrase là Hey Liva', async () => {
  const source = await readRepoFile('liva-ui/src/WidgetApp.vue')

  assert.match(source, /Câu gọi duy nhất: “Hey Liva”/u)
  assert.doesNotMatch(source, /Này Liva|Liva ơi/u)
})

test('trang thử microphone dùng câu bắt đầu bằng Hey Liva và không gọi energy là wake', async () => {
  const [html, script] = await Promise.all([
    readRepoFile('liva-ui/public/wake-word-test.html'),
    readRepoFile('liva-ui/public/wake-word-test.js'),
  ])

  assert.match(html, /Hey Liva, bật nhạc lên giúp tôi/u)
  assert.match(html, /chỉ đo năng lượng, không xác minh từ khóa/u)
  assert.doesNotMatch(script, /WAKE WORD DETECTED/u)
  assert.match(script, /Đang lắng nghe câu bắt đầu bằng “Hey Liva”/u)
})

test('README beta cấu hình voice wake mode và pipeline âm thanh', async () => {
  const readme = await readRepoFile('README.md')

  // Các giá trị và mặc định theo `liva-native-core/src/wake.rs` (WakeGate::from_env).
  assert.match(
    readme,
    /Voice wake mode: "off" \(default\), "asr_prefix", "trained_model" or "hybrid"/u,
  )
  assert.match(readme, /\$env:LIVA_WAKE_MODE = "asr_prefix"/u)
})
