---
title: "Resource governor — chính sách sống chung với game và workload nặng"
updated: 2026-08-07
commit: bd11c84
status: living
owns:
  - nguong-governor
  - resource-coexistence-policy
covers:
  - liva-native-core/src/governor.rs
  - liva-native-core/src/boot.rs
  - liva-native-core/src/lib.rs
  - liva-native-core/src/vision/capture.rs
  - liva-native-core/src/llm/engine.rs
  - liva-native-core/src/sysinfo.rs
stale-ok: a0153135
---
# Resource governor — chính sách sống chung với game và workload nặng

[⬆ Mục lục](../README.md) · [Vision runtime](../03-he-thong-con/vision.md) ·
[Runtime native](../01-ban-ve/01-kien-truc-tong-the.md) ·
[Master roadmap](../06-ke-hoach/roadmap.md)

## 1. Kết luận as-built

Governor có hai cơ chế liên quan nhưng **không dùng cùng tín hiệu**:

1. `Governor::game_mode_active` hạ priority tiến trình khi có cửa sổ ngoài LIVA fullscreen,
   CPU ngoài LIVA vượt ngưỡng hoặc GPU ngoài LIVA vượt ngưỡng.
2. watcher GPU-layer và vision auto-crop dùng `game_mode_active_now`, chỉ xét fullscreen trong
   mode `auto`.

Vì vậy LIVA hiện nhường CPU cho cả workload không-fullscreen, nhưng chỉ trả GPU layer/crop ảnh tự
động khi phát hiện fullscreen. Đây là ranh giới sản phẩm phải giữ đúng trong UI và tài liệu.

## 2. Tín hiệu và ngưỡng

| Biến | Mặc định | Ý nghĩa |
|---|---:|---|
| `LIVA_GAME_MODE` | `auto` | `on` ép bật; `off` tắt; giá trị khác dùng auto |
| `LIVA_GAME_PRIORITY` | bật | `off` vô hiệu hạ priority |
| `LIVA_BUSY_CPU_PERCENT` | 80 | 0 tắt nhánh CPU; rác hoặc >100 về mặc định |
| `LIVA_BUSY_GPU_PERCENT` | 80 | 0 tắt nhánh GPU; rác hoặc >100 về mặc định |
| `LIVA_GAME_N_GPU_LAYERS` | 0 | số GPU layer khi watcher fullscreen chuyển mode |
| `LIVA_LLM_N_GPU_LAYERS` | vắng = tự chọn 99/0 | override số layer bình thường; tự chọn dựa VRAM trống + kích thước router/mmproj + 2 GiB dự phòng |

`liva-native-core/src/governor.rs#external_cpu_percent` tính tải của tiến trình **khác** bằng
`GetSystemTimes`, rồi trừ CPU-time của chính LIVA từ `GetProcessTimes`. Việc trừ này tránh vòng
phản hồi “LLM làm CPU cao → governor tự bóp LIVA”.

`liva-native-core/src/governor.rs#external_gpu_percent` áp dụng nguyên tắc tương tự:

- biết utilization của process LIVA thì trừ;
- không biết mà LIVA có GPU layer thì bỏ tín hiệu (`None`) thay vì đoán;
- không biết và LIVA CPU-only thì dùng utilization tổng.

GPU metric dùng NVML nạp động. Máy không có NVIDIA/driver hoặc môi trường WDDM không cho tách
process có thể làm nhánh GPU không có số đo; fullscreen và CPU vẫn hoạt động.

## 3. Hạ priority tiến trình

`liva-native-core/src/governor.rs#Governor::game_mode_active` cache kết quả 2 giây và quyết định:

```text
active = fullscreen OR external_cpu >= cpu_threshold OR external_gpu >= gpu_threshold
```

Khi active đổi trạng thái, `apply_priority` đặt process Windows xuống `BELOW_NORMAL`; khi hết bận
thì trả `NORMAL`. `LIVA_GAME_PRIORITY=off` giữ nguyên priority dù detector vẫn trả trạng thái.

`liva-native-core/src/boot.rs#spawn_background_services` chạy vòng kiểm này trên một OS thread mỗi
5 giây. Cả vỏ Tauri và standalone dùng cùng hàm boot, nên không có khác biệt profile ở cơ chế này.

## 4. Chuyển GPU layer

Background service thứ tư trong `boot::spawn_background_services` so số layer bình thường đã
resolve (env hoặc `gpu_layers_mac_dinh`) với `LIVA_GAME_N_GPU_LAYERS`:

- số layer bình thường bằng 0 hoặc hai giá trị bằng nhau: không spawn watcher có tác dụng;
- mỗi 5 giây kiểm fullscreen bằng `game_mode_active_now`;
- chỉ reload khi trạng thái đổi;
- nếu model chưa nạp, không chốt trạng thái và thử lại nhịp sau.

`liva-native-core::reload_llm_gpu_layers` nạp lại model ở số layer mục tiêu. Reload có
chi phí giây và xóa KV cache; vì vậy không được gọi ở mọi poll hoặc theo dao động metric ngắn.

Điểm cần nói thẳng: CPU/GPU threshold **không** kích hoạt GPU-layer watcher hiện tại. Muốn hợp
nhất tín hiệu phải thiết kế hysteresis/debounce riêng và đo reload churn trước khi sửa.

## 5. Ảnh hưởng lên Vision

`liva-native-core/src/vision/capture.rs#capture_for_vision` dùng cùng hàm tức thời
`game_mode_active_now` cho `LIVA_VISION_REGION=auto`. Khi fullscreen, ảnh được crop quanh con trỏ;
khi chỉ CPU/GPU bận nhưng không fullscreen, ảnh vẫn là toàn màn hình.

Đây là lựa chọn footprint, không phải security boundary. Crop quanh con trỏ vẫn có thể chứa dữ
liệu nhạy cảm và vẫn phải tuân theo ranh giới ở [Vision runtime](../03-he-thong-con/vision.md).

## 6. Failure mode và giới hạn

| Trường hợp | Hành vi |
|---|---|
| Windows API lấy CPU lỗi/lần mẫu đầu | CPU signal `None`; thử lại sau |
| Không có NVML/NVIDIA | GPU signal `None`; không làm hỏng CPU/fullscreen |
| Không tách được GPU process, LIVA dùng GPU | bỏ tín hiệu để tránh tự throttle |
| YouTube/slide fullscreen | có thể dương tính giả và bật chế độ tiết kiệm |
| Render CPU/GPU cửa sổ thường | hạ priority có thể bật; GPU layer/crop không đổi |
| Desktop shell hoặc cửa sổ LIVA fullscreen | bị loại khỏi fullscreen detector |
| Nhiều màn hình | fullscreen detector so primary screen |

Governor hiện là heuristic cục bộ, chưa phải scheduler có SLO. Không có memory-pressure signal,
battery/thermal policy, per-task budget hay deadline-aware preemption.

## 7. Acceptance và hướng nâng cấp

Các cổng tối thiểu:

```powershell
cargo test -p liva-native-core governor
cargo test -p liva-native-core --lib boot
cargo clippy --workspace --all-targets -- -D warnings
```

Lát nâng cấp tiếp theo chỉ được nhận khi có benchmark:

1. log state transition và reload duration, không log nội dung người dùng;
2. thêm hysteresis cho CPU/GPU để tránh dao động quanh ngưỡng;
3. đo frame-time/game workload trước và sau;
4. quyết định tường minh có hợp nhất detector cho priority, GPU layer và vision crop hay không;
5. test không reload lặp khi model chưa nạp, đang đúng target hoặc tín hiệu chập chờn.
