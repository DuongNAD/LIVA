import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'

const configPath = 'tools/wakeword/hey_liva_prod.yaml'
const toolchainPath = 'tools/wakeword/toolchain.json'
const config = readFileSync(configPath, 'utf8')
const toolchain = JSON.parse(readFileSync(toolchainPath, 'utf8'))

function numberAt(key) {
  const match = config.match(new RegExp(`^${key}:\\s*([0-9.]+)\\s*$`, 'mu'))
  assert.ok(match, `${configPath}: thiếu ${key}`)
  return Number(match[1])
}

assert.equal(
  toolchain.repository,
  'https://github.com/livekit/livekit-wakeword.git',
  'training toolkit phải dùng repository chính thức',
)
assert.match(toolchain.commit, /^[0-9a-f]{40}$/u, 'training toolkit phải pin commit SHA đầy đủ')
assert.match(config, /^model_name:\s*wake_liva_en_v2$/mu)
assert.match(config, /^\s*-\s*"hey liva"\s*$/mu)
assert.match(config, /^\s*model_type:\s*conv_attention\s*$/mu)
assert.match(config, /^\s*model_size:\s*medium\s*$/mu)
assert.ok(numberAt('n_samples') >= 25_000, 'production model cần ít nhất 25k mẫu mỗi lớp')
assert.ok(numberAt('n_samples_val') >= 5_000, 'validation cần ít nhất 5k mẫu mỗi lớp')
assert.ok(numberAt('n_background_samples') >= 2_000, 'background corpus tổng hợp quá nhỏ')
assert.ok(numberAt('steps') >= 100_000, 'production training cần ít nhất 100k bước')
assert.ok(numberAt('target_fp_per_hour') <= 0.1, 'training target phải <= 0.1 FPPH')
assert.match(config, /^\s*-\s*"hey diva"\s*$/mu, 'thiếu adversarial negative gần âm')
assert.match(config, /^data_dir:\s*\.\/tools\/wakeword\/work\/data$/mu)
assert.match(config, /^output_dir:\s*\.\/tools\/wakeword\/work\/output$/mu)

console.log(
  `wake training config: PASS (${configPath}, livekit-wakeword@${toolchain.commit.slice(0, 12)})`,
)
