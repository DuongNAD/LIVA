# Kế hoạch implement — làm avatar 3D LIVA tự nhiên, mượt và tương tác tốt hơn

> **Nguồn tham khảo.** `3d-realistic-character-walk-&-physics-studio.zip` (Google AI Studio applet,
> Next.js 15 + React 19 + `three@0.185.1`, 30 file / 380 KB). Đã giải nén và đọc toàn bộ.
> **Đối chiếu với mã LIVA tại HEAD `67eb98b`** — mọi trích dẫn `file:dòng` bên dưới đã tự kiểm trong phiên.
>
> **Kết luận ngắn:** ~95% dự án tham khảo **không dùng được** cho LIVA (lý do ở §1).
> Nhưng khối `moreHuman` trong `lib/procedural-character.ts:330-345` là **đúng thứ LIVA đang thiếu**,
> và nó ghép vào được với chi phí rất thấp — vì nó chạm đúng những xương mà LIVA **không** điều khiển.

---

## 1. Dự án tham khảo — cái gì dùng được, cái gì không

| Thành phần | Kích thước | Dùng được cho LIVA? |
|---|---:|---|
| `lib/procedural-character.ts` — khối `moreHuman` | ~15 dòng | ✅ **Đây là toàn bộ giá trị.** Xem §3 |
| `lib/procedural-character.ts` — phần dựng mesh | ~230 dòng | ❌ Nhân vật ghép từ khối trụ/cầu/hộp. LIVA có model VRM thật |
| `components/ThreeScene.tsx` — controller WASD, camera-relative, gravity, cầu thang, va chạm prop | 1 474 dòng | ❌ LIVA là **companion trên overlay desktop**, không phải game nhân vật đi trong phòng |
| `components/MobileJoystick.tsx`, `UIOverlay.tsx`, `RedditContextModal.tsx` | ~56 KB | ❌ Không liên quan |
| `lib/audio.ts` — tiếng bước chân, tiếng va chạm | 6,8 KB | ❌ Trợ lý desktop phát tiếng bước chân = phiền |
| Bộ đèn 4 preset + `ACESFilmicToneMapping` | — | ◐ Một phần — xem §6 |

**Một điểm dự án tham khảo làm *kém hơn* LIVA, đừng bê sang:**

```ts
// ThreeScene.tsx:1186
currentHorizVel.lerp(targetHorizVel, Math.min(1.0, 10.0 * delta));
```

Lerp hệ số nhân thẳng với `delta` — **phụ thuộc frame rate**, chạy 120 FPS mượt hơn 30 FPS.
LIVA đã làm đúng ở `use3DModel.ts:832` (`1 - Math.pow(0.005, delta)`) và ở `footPlantIK.ts:52`
(`1 - Math.exp(-response * delta)`). Giữ nguyên cách của LIVA.

---

## 2. LIVA hiện có gì — đã tự kiểm

**Tầng "hiện diện" đã tốt, không cần đụng:** auto-blink ease-out + 20% double-blink, lookAt
spring-damped, thở sine 4 giây, micro-sway OpenSimplex 2 tầng, micro-expressions, lip-sync ưu tiên
audio. Vòng lặp đặt pose **trước** `vrm.update()` để spring bone tóc/váy phản ứng ngay trong frame
(`use3DModel.ts:838-871`), và substep physics theo bội số 1/60 s (`use3DModel.ts:865-869`). Tốt.

**Tầng "thân thể" là chỗ yếu, và đây là số đo:**

```ts
// mixamoRetarget.ts:2-8 — đúng 11 xương
export const CONTROLLED_BONES = [
  "leftUpperLeg","leftLowerLeg","leftFoot","rightUpperLeg","rightLowerLeg","rightFoot",
  "leftUpperArm","leftLowerArm","rightUpperArm","rightLowerArm","hips",
] as const;

// mixamoRetarget.ts:14 — chỉ có góc xoay, KHÔNG có track vị trí
export type Pose = Partial<Record<ControlledBone, Euler3>>;
```

Không có **spine, chest, neck, head, shoulder, hand, finger**, và **không có vị trí hips**.
Tài liệu ghi mỗi FBX Mixamo chứa 68 tên xương `mixamorig:` (đo 07/08/2026) — tức đang giữ 11/68.

Hệ quả nhìn thấy: **thân trên đứng im như tượng khi avatar đi.** Không vai đánh theo, không spine
phản xoay, đầu không giữ thăng bằng. Đây là nguyên nhân "trông giả" lớn hơn nhiều so với cái khựng
mà [U30](docs/03-danh-gia/05-nang-cap-toan-dien.md) đang đuổi.

**Một chi tiết quan trọng ít ai để ý:** LIVA **đã có sẵn** một lớp procedural chạy song song với clip —
`useAvatarAnimation.ts:420-423`:

```ts
const procedural = basePose(poseState, clock, stridePhase, motionWeight);
const clip = clips.get(poseState);
if (!clip) return procedural;
return { ...procedural, ...sampleRetargetedClip(clip, time, poseState !== "jump") };
```

Clip ghi đè procedural trên 11 xương nó có; procedural lấp phần còn lại. **Cả hai đều chỉ phủ 11 xương.**
Và `stridePhase` đã tồn tại sẵn ở `useAvatarAnimation.ts:273` — nghĩa là **không phải kéo dây gì cả**,
chỗ để cắm lớp phụ trợ mới đã có đủ nguyên liệu.

---

## 3. Thay đổi A1 — Lớp phụ trợ thân trên (giá trị cao nhất, rủi ro thấp nhất)

### Vì sao an toàn: nó chạm đúng những xương LIVA đang bỏ trống

Khối `moreHuman` của dự án tham khảo (`procedural-character.ts:330-345`):

```ts
root.position.y   = 0.95 - Math.abs(Math.sin(cycle * 2)) * (isRunning ? 0.09 : 0.04);
root.rotation.z   = Math.sin(cycle) * 0.04;          // hip roll
root.rotation.y   = Math.sin(cycle) * 0.05;          // hip yaw
spineGroup.rotation.y = -Math.sin(cycle) * 0.08;     // spine phản xoay
spineGroup.rotation.x = isRunning ? 0.2 : 0.06;      // ngả người tới
headGroup.rotation.x  = -spineGroup.rotation.x * 0.6; // giữ tầm mắt ngang
headGroup.rotation.y  = Math.sin(cycle * 0.5) * 0.03;
```

⚠️ **KHÔNG bê nguyên khối này.** Phải tách làm hai nhóm, vì LIVA có `FootPlantIK` còn dự án kia thì không:

| Dòng | Xương | Có làm chân xê dịch? | Kết luận |
|---|---|---|---|
| `spineGroup.rotation.y` (phản xoay) | spine | Không — spine nằm **trên** hips | ✅ **Lấy** |
| `spineGroup.rotation.x` (ngả người) | spine | Không | ✅ **Lấy** |
| `headGroup.rotation.x/y` | head | Không | ✅ **Lấy** |
| `root.rotation.z/y` (hip roll/yaw) | root | **Có** — xoay cả thân kể cả chân | ⚠️ Hoãn, xem §5 |
| `root.position.y` (nhấp nhô) | root | **Có** | ❌ **Đừng làm** — xem ngay dưới |

**Vì sao `root.position.y` sẽ bị vô hiệu hoá, không phải lý thuyết:** `footPlantIK.ts:88-92` trả về
`y: clamp(anchor.y - lockedPoint.y)`. Nếu ta đẩy pelvis lên xuống, bàn chân đang khoá đi theo, IK thấy
chân rời neo và **bù ngược đúng bằng lượng ta vừa thêm** (kẹp ở ±0,14). Kết quả: công cốc, cộng thêm rung.

Sâu hơn: **nhấp nhô của dáng đi thật là *hệ quả* của việc duỗi chân, không phải một offset cộng vào.**
Dự án tham khảo không có IK nên bù giả là hợp lý ở đó. LIVA có IK nên bù giả là sai ở đây.
Đây chính là cùng một lỗi mà [U30](docs/03-danh-gia/05-nang-cap-toan-dien.md) đã trả giá với bù ngang.

### Việc cụ thể

**File:** `liva-ui/src/composables/use3DModel.ts` · `liva-ui/src/composables/useAvatarAnimation.ts`

1. `useAvatarAnimation.ts` — expose `stridePhase` và `motionWeight` hiện tại ra ngoài qua API trả về
   (đã có sẵn ở dòng 273/406, chỉ cần đọc ra).
2. `use3DModel.ts` — viết `updateLocomotionUpperBody(delta, phase, motionWeight, isRunning)`:
   - `spine.rotation.y += -Math.sin(phase) * 0.08 * motionWeight`
   - `spine.rotation.x += (isRunning ? 0.2 : 0.06) * motionWeight`
   - `head.rotation.x += -spine.rotation.x * 0.6`
   - `head.rotation.y += Math.sin(phase * 0.5) * 0.03 * motionWeight`
   - Nhân `motionWeight` để khi đứng yên lớp này tự tắt, không cần rẽ nhánh.
3. Gọi nó ngay sau `animation.update()` (`use3DModel.ts:841`) và **trước** `vrm.update()`.

**Cạm bẫy phải xử cùng commit — xem A3.** `updateIdle()` đang **gán đè** chứ không cộng.

**Nghiệm thu.** Mở `npm run dev -w liva-ui`, cho avatar đi, quay 10 giây đặt cạnh bản cũ:
thân trên **có** chuyển động, vai đánh theo bước, đầu giữ tầm ngang khi thân ngả. Đây là nghiệm thu
**bằng mắt** — cùng loại với U30, và `requestAnimationFrame` treo khi khung nhìn ẩn nên không tự động hoá được.

---

## 4. Thay đổi A2 — Phase theo quãng đường, không theo thời gian

Đây là mục mà backlog xếp vào [U33](docs/03-danh-gia/05-nang-cap-toan-dien.md) và ước lượng "nhiều tuần".
**Riêng phần lõi của nó là 2 dòng**, và dự án tham khảo vô tình làm đúng:

```ts
// procedural-character.ts:243
animTime += delta * (speed > 0.1 ? speed * 3.2 : 2.0);
```

`delta × speed` = **quãng đường**. Tức phase tiến theo đoạn đường đã đi, không theo đồng hồ.

LIVA hiện làm theo thời gian (`useAvatarAnimation.ts:404-407`):

```ts
const hz = STRIDE_HZ[state] || STRIDE_HZ[previousState];   // walk 1.05, run 1.9
stridePhase = (stridePhase + delta * hz * motionWeight * Math.PI * 2) % (Math.PI * 2);
```

Nhịp cố định theo giây ⇒ khi tốc độ đổi mà nhịp không đổi, **chân trượt trên sàn**. Đó là lý do phải có
`FootPlantIK` bù lại. Sửa tận gốc:

```ts
// STRIDE_LENGTH: quãng đường mỗi bước (mét), hằng số cho từng trạng thái
stridePhase = (stridePhase + (currentSpeed * delta) / STRIDE_LENGTH[state] * Math.PI * 2) % (Math.PI * 2);
```

`useAvatarLocomotion.ts` **đã có sẵn** `speed`, `WALK_SPEED`/`RUN_SPEED` và cả gia/giảm tốc
(`useAvatarLocomotion.ts:182-185`) — chỉ cần truyền tốc độ hiện tại vào.

**Hiệu chuẩn `STRIDE_LENGTH`:** đặt sao cho ở đúng `WALK_SPEED` thì nhịp ra bằng `STRIDE_HZ.walk = 1.05`,
tức `STRIDE_LENGTH.walk = WALK_SPEED / 1.05`. Như vậy ở tốc độ danh nghĩa dáng đi **không đổi gì cả** —
thay đổi chỉ lộ ra khi tăng/giảm tốc, đúng chỗ đang trượt.

**Nghiệm thu.** Đo lượng trượt của chân trụ (px/frame) trong lúc gia tốc `idle → walk → run`:
phải **giảm** so với bản cũ. Và `FootPlantIK` phải trả về `y` nhỏ đi — IK càng ít phải bù thì phase càng đúng.
`npm run test:coverage -w liva-ui` không tụt ngưỡng nào.

---

## 5. Thay đổi A3 — Một điểm hợp nhất tư thế (bắt buộc, làm cùng A1)

**Đây là cạm bẫy sẽ cắn ngay ở dòng code đầu tiên của A1.** `use3DModel.ts:903-921`:

```ts
spine.rotation.x = breathCycle;   // :908  — GÁN, không cộng
head.rotation.x  = swayX;         // :918  — GÁN
head.rotation.y  = swayY;         // :919  — GÁN
```

Hôm nay vô hại vì locomotion không đụng spine/head. Nhưng A1 sẽ ghi vào **đúng hai xương đó**, và
`updateIdle()` chạy **sau** `animation.update()` — nên nó sẽ **xoá sạch** phần A1 vừa ghi, mỗi frame.

Triệu chứng khi đó: *"đã thêm lớp thân trên mà vẫn không thấy gì nhúc nhích"* — đọc y hệt "A1 không ăn thua",
và sẽ không ai nghĩ tới nhịp thở. **Đây là loại lỗi tốn nguyên buổi nếu không biết trước.**

**Cách sửa — gộp về một chỗ, theo thứ tự rõ ràng:**

```
1. animation.update(vrm, delta)         → tư thế gốc (11 xương, clip + procedural)
2. đặt lại spine/head về rest           → điểm gốc xác định cho các lớp cộng
3. += lớp locomotion thân trên (A1)     → chỉ khi motionWeight > 0
4. += lớp thở + micro-sway (updateIdle) → chuyển từ GÁN sang CỘNG
5. lookAt spring-damped, blink, lip-sync
6. vrm.update(delta)                    → spring bone tóc/váy
```

Bước 2 quan trọng: không có nó thì các lớp cộng dồn vô hạn qua từng frame.

**Nghiệm thu.** Một unit test khoá thứ tự: dựng VRM giả, chạy 1 frame với `motionWeight = 0` →
spine chỉ có thành phần thở; chạy với `motionWeight = 1` → spine có **cả hai** thành phần
(giá trị khác cả hai trường hợp đơn lẻ). Đảo thứ tự bước 3/4 thì test phải **đỏ**.

---

## 6. Thay đổi A4 — Rim light (rẻ nhất, hiệu quả thị giác cao nhất)

LIVA hiện có ambient + hemisphere + directional key + fill (`use3DModel.ts:377-390`) — **không có rim light**.

Dự án tham khảo có (preset neon: `rimPink` ở `ThreeScene.tsx:415`). Với LIVA thì đây **quan trọng hơn**
với nó, vì avatar LIVA là **overlay trong suốt nằm trên nội dung desktop bất kỳ** — nền có thể là IDE tối,
trình duyệt sáng, game rực rỡ. Một đèn viền sau lưng là thứ tách nhân vật khỏi nền trong mọi trường hợp.

**Việc.** Thêm một `DirectionalLight` cường độ thấp đặt phía sau-trên avatar, hướng về camera.
Cân nhắc `ACESFilmicToneMapping` — nhưng **đo trước**: tone mapping đổi toàn bộ tông màu model,
có thể xung đột với material do VRM tác giả chỉnh sẵn.

**Nghiệm thu.** Ảnh chụp avatar trên 3 nền (tối / sáng / nhiều màu), so trước-sau. Không đổi `pixelRatio`,
không đổi `antialias` — đây là thay đổi ánh sáng thuần.

---

## 7. Thứ tự thi hành

| Lát | Việc | Công sức | Nghiệm thu |
|---|---|---|---|
| **0** | Đóng nhánh chẩn đoán [U30](docs/03-danh-gia/05-nang-cap-toan-dien.md): mở dev, bật/tắt `LIVA_FOOT_PLANT`, **nhìn** | 5 phút | Mắt người. Chỉ bạn làm được |
| **1** | **A3** (điểm hợp nhất) + **A1** (lớp thân trên) — **một commit** | 0,5–1 ngày | Video 10 s đặt cạnh bản cũ; test khoá thứ tự |
| **2** | **A4** (rim light) | 1 giờ | 3 ảnh chụp trên 3 nền |
| **3** | **A2** (phase theo quãng đường) | 0,5 ngày | Trượt chân giảm; `FootPlantIK.y` nhỏ đi |
| **4** | Đánh giá lại: A1 đã đủ tự nhiên chưa? | — | Nếu đủ thì **U32 không cần làm** |

Lát 0 phải trước lát 1 vì nếu tắt `LIVA_FOOT_PLANT` mà vẫn khựng thì chẩn đoán U30 sai chỗ,
và điều đó đổi thứ tự phần còn lại.

Lát 4 là điểm quyết định thật: **A1 rẻ hơn [U32](docs/03-danh-gia/05-nang-cap-toan-dien.md) khoảng một
bậc độ lớn** (1 ngày so với 2–4 ngày viết lại retarget sang `AnimationClip`/`AnimationMixer`). Nếu A1
đã cho cảm giác đủ sống, U32 chuyển từ "cần làm" sang "làm khi rảnh". Đừng nhận U32 trước khi đo A1.

---

## 8. Những thứ KHÔNG làm

- **Đừng bê mesh procedural.** LIVA có VRM thật với 456 morph target. Thay bằng khối trụ/cầu là đi lùi.
- **Đừng bê controller WASD / gravity / va chạm prop.** LIVA không phải game; avatar di chuyển theo
  toạ độ màn hình qua `useAvatarLocomotion.moveTo()`, và `MAX_TURN = 0.9 rad (~52°)` là **cố ý** —
  avatar luôn hơi hướng về phía người dùng (`use3DModel.ts:274-275`).
- **Đừng bê lerp phụ thuộc frame rate** (`ThreeScene.tsx:1186`). LIVA đã làm đúng hơn.
- **Đừng thêm `root.position.y` nhấp nhô** — `FootPlantIK` sẽ triệt tiêu nó. Xem §3.
- **Đừng làm Motion Matching / PFNN** — dự án có đúng 6 clip; §8 của backlog đã chốt.
- **Đừng hạ frame rate avatar khi người dùng đang nhìn** — §8 backlog.
- **Đừng bật `combineMorphs`** — có test khẳng định nó chưa được gọi, để lần bật sau là quyết định có ý thức.

---

## 9. Trục "tương tác" — không nằm ở animation, nhưng đáng làm hơn cả [HOÀN THÀNH]

Ba thứ này ảnh hưởng cảm nhận mạnh hơn mọi thay đổi ở §3–§6, và đều là kỹ thuật thuần
(đúng nguyên tắc chọn việc ở nhóm F backlog: không đặt cược vào việc model trả lời hay):

1. **Phản ứng thân thể *trước* token đầu tiên.** [ĐÃ HOÀN THÀNH]
   - TTFT p50 đo được là 667 ms (CPU) / 18 ms (CUDA).
   - Đã triển khai: khi nhận sự kiện `ai_thinking_start`, avatar ngước nhìn camera (`updateLookAt(0, 4)`) và nghiêng đầu nhẹ (`head.rotation.z += 0.10 * thinkingWeight; head.rotation.x += 0.04 * thinkingWeight; head.rotation.y += -0.03 * thinkingWeight;`), che lấp hoàn hảo độ trễ suy luận.
2. **Barge-in phải thấy được trên thân thể.** [ĐÃ HOÀN THÀNH]
   - Đã triển khai: khi phát hiện cướp lời (`audio_ducking < 0.6` hoặc `ai_stream_reset` / `ai_thinking_start` khi đang phát âm thanh), avatar lập tức đóng miệng (`stopAudioDrivenLipSync()` + triệt tiêu các viseme morphs), ngước mắt nhìn thẳng người dùng (`updateLookAt(0, 0)`), và kích hoạt biểu cảm ngạc nhiên/chú ý (`surprised: 0.45` trong 350ms).
3. **Gaze bám vùng đang nhìn.** [ĐÃ HOÀN THÀNH]
   - Đã triển khai: backend LIVA Native Core lấy toạ độ con trỏ chuột chuẩn hoá gửi sự kiện WebSocket `vision_inspect` `{ x, y }`, avatar quay đầu và hướng mắt bám theo điểm nhìn (`inspectScreenPoint(x, y)`), và tự động hoàn trả góc nhìn về camera khi backend gửi `vision_inspect_clear`.

---

*Lập 18/08/2026 · đối chiếu mã LIVA tại `67eb98b` · Mục 9 nghiệm thu hoàn tất 07/09/2026.*
