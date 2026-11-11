# Mục tiêu cốt lõi, hướng đi và lộ trình của LIVA

Trạng thái: **Bản nháp**, viết ngày 2026-11-11 theo yêu cầu của chủ dự án. Chưa mục nào được duyệt. Tài liệu này không thay đổi mã nguồn.

Đây là tài liệu kế hoạch duy nhất đang hiệu lực. Các kế hoạch cũ nằm trong [`docs/99-luu-tru/`](docs/99-luu-tru/README.md) và đã đóng băng.

## 1. Tóm tắt

- LIVA là trợ lý cá nhân chạy trên máy của người dùng: nghe, nói, nhìn, ghi nhớ và làm việc trên máy tính.
- Hôm nay người ngoài gần như không dùng được: chưa có bản cài nào được phát hành, và mô hình mặc định chạy chậm trên máy không có GPU.
- Hướng đi: thu hẹp về vài việc làm chắc, cho phép mô hình chạy ở một máy khác, rồi phát hành bản cài.
- LIVA không tự huấn luyện mô hình. Khi có mô hình mở tốt hơn, LIVA đổi mô hình và tốt lên theo.
- Mọi mục trong lộ trình ở trạng thái Kế hoạch cho tới khi chủ dự án duyệt.

## 2. Mục đích

Một câu, dùng chung cho `README.md`, mô tả trên GitHub và tài liệu này:

> LIVA là trợ lý cá nhân local-first cho Windows: nghe, nói, nhìn, ghi nhớ và làm việc trên máy tính, với dữ liệu ở lại trên máy của người dùng.

Người dùng đầu tiên là người có một laptop phổ thông và muốn một trợ lý hiểu tiếng Việt mà không gửi dữ liệu đi nơi khác. Đây là giả thuyết, chưa có khảo sát nào kiểm chứng (xem KH-10).

## 3. Hiện trạng

Kiểm tại `origin/main` `3057d35c`, ngày 2026-11-11.

| Mục | Sự thật | Nguồn |
|---|---|---|
| Hệ điều hành | Chỉ Windows 10/11 x64 | `README.md`, mục System Requirements |
| Bản phát hành | GitHub chưa có release nào; thẻ duy nhất là `demo-2026-07` | `CHANGELOG.md` |
| Cài từ mã nguồn | Cần Rust 1.85+, Node 20+, Visual Studio 2022 C++, CMake, LLVM | `README.md`, mục System Requirements |
| Phần cứng | Tối thiểu 16 GB RAM; khuyến nghị GPU NVIDIA 6 GB VRAM trở lên | `README.md`, mục System Requirements |
| Mô hình phải tải | Bộ tối thiểu khoảng 5,6 GB; bộ đầy đủ khoảng 11,6 GB | [`models/README.md`](models/README.md) |
| Mô hình chat mặc định | `gemma-4-E4B-it-qat` (4,2 GB), chạy ngay trong tiến trình qua llama.cpp | `data/liva-config.json`, `data/models-manifest.json`, `liva-native-core/Cargo.toml` |
| Tốc độ trên CPU | Thời gian tới token đầu: p50 4 874 ms, từ 2 310 ms tới 13 476 ms qua 10 lượt; sau đó khoảng 1,5 token mỗi giây | [bản ghi đo, tháng 8 năm 2026](docs/99-luu-tru/bao-cao-lich-su/05-nang-cap-toan-dien.md) (lưu trữ) |
| Mô hình ở máy khác | Chưa làm được. Tiến trình `llama-server` phụ chỉ nghe ở `127.0.0.1`, và lõi Rust chưa đọc khoá cấu hình `cloudBaseUrl` lúc chạy | `liva-native-core/src/llm/sidecar.rs`, `liva-native-core/src/commands/config.rs` |
| LIVA phục vụ mô hình cho công cụ khác | Có, tắt mặc định: bề mặt HTTP tương thích OpenAI (`/v1/models`, `/v1/chat/completions`, `/v1/audio/speech`) bật bằng `LIVA_OPENAI_PORT`. Bề mặt này không có xác thực | `liva-native-core/src/openai_api.rs`, `.env.example` |
| Giấy phép | MIT hoặc Apache-2.0 (đổi ngày 2026-11-11, chờ gộp) | `LICENSE-MIT`, `LICENSE-APACHE` |

## 4. Vấn đề cần giải

1. **Không có đường cài cho người ngoài.** Muốn thử LIVA phải tự build với bộ công cụ C++ đầy đủ.
2. **Máy phổ thông không đủ mạnh.** Trên CPU, một câu trả lời có khi bắt đầu sau 2 giây, có khi sau 13 giây. Với trợ lý thoại, độ dao động đó là thứ người dùng cảm nhận rõ nhất.
3. **Phạm vi rộng so với sức một người.** Giọng nói, thị giác, bộ nhớ, tự động hoá máy tính và avatar được làm cùng lúc. Tài liệu này đề xuất chọn ba việc để kiểm kỹ trước (KH-7, KH-8).
4. **Mô hình nhỏ yếu ở chuỗi hành động nhiều bước.** Đây là nhận định chung về mô hình cỡ này, chưa được đo trong LIVA; KH-8 sẽ đo.

## 5. Hướng đi

- **H1. Vài việc làm chắc.** Chọn ba việc mà mô hình mặc định làm ổn, và kiểm chúng bằng kịch bản lặp lại được. Các năng lực khác vẫn ở trong mã nhưng không được quảng bá cho tới khi qua kiểm.
- **H2. Mô hình có thể ở máy khác.** Laptop lo giao diện, giọng nói và bộ nhớ. Mô hình ngôn ngữ chạy trên một máy mạnh hơn của chính người dùng, hoặc ở một dịch vụ ngoài nếu người dùng tự chọn. Mặc định vẫn là chạy tại chỗ.
- **H3. Người ngoài cài được.** Một bộ cài tải từ trang Releases, kèm yêu cầu phần cứng nói thật.
- **H4. Đổi mô hình là việc cấu hình.** LIVA không gắn với một mô hình. Khi có mô hình mở nhẹ và tốt hơn, chỉ cần đổi manifest và đo lại.

## 6. Công nghệ

| Lớp | Công nghệ đang dùng | Nguồn |
|---|---|---|
| Vỏ ứng dụng | Tauri v2 | `Cargo.toml` |
| Giao diện | Vue 3, Three.js, Vite | `liva-ui/package.json` |
| Lõi | Rust, Tokio | `Cargo.toml` |
| Mô hình ngôn ngữ | llama.cpp qua crate `llama-cpp-2`; có đặc tính `cuda` và `vulkan` | `liva-native-core/Cargo.toml` |
| Mô hình tín hiệu và nhúng | ONNX Runtime qua crate `ort` | `Cargo.toml` |
| Lưu trữ | SQLite qua crate `rusqlite` | `Cargo.toml` |
| Giao tiếp với công cụ ngoài | HTTP tương thích OpenAI, tắt mặc định | `liva-native-core/src/openai_api.rs` |

Lựa chọn cho H2: dùng giao thức tương thích OpenAI cho mô hình ở xa. LIVA đã nói giao thức này ở chiều phục vụ, và `llama-server`, tiến trình phụ mà LIVA đang dùng, cũng nói được nó. Vì vậy không cần một giao thức riêng.

## 7. Lộ trình

Trạng thái dùng bốn từ: Đã có (Implemented), Một phần (Partial), Kế hoạch (Planned), Chỉ thiết kế (Design only).

### Giai đoạn 1: chạy được trên máy yếu

| Mã | Việc | Xong khi | Trạng thái |
|---|---|---|---|
| KH-1 | Trỏ mô hình chat tới một endpoint tương thích OpenAI, đặt trong cấu hình | LIVA trên laptop không có GPU trả lời bằng mô hình chạy ở máy khác. Thời gian tới token đầu được đo 10 lượt và ghi lại. Mất kết nối thì báo lỗi rõ, không treo | Kế hoạch |
| KH-2 | Hướng dẫn hai máy: máy mạnh phục vụ mô hình, laptop trỏ tới | Một người làm theo hướng dẫn từ đầu tới câu trả lời đầu tiên, có ghi thời gian | Kế hoạch |
| KH-3 | Giao diện nói rõ khi câu hỏi rời khỏi máy | Mỗi khi mô hình ở xa được dùng, người dùng nhìn thấy điều đó trên màn hình | Kế hoạch |

### Giai đoạn 2: người ngoài cài được

| Mã | Việc | Xong khi | Trạng thái |
|---|---|---|---|
| KH-4 | Phát hành bộ cài đầu tiên trên GitHub Releases | Một người chưa từng build tải bộ cài từ trang Releases và tới được câu trả lời đầu tiên | Kế hoạch |
| KH-5 | `README.md` nêu yêu cầu phần cứng và dung lượng mô hình theo số đã đo | Mỗi con số trong `README.md` dẫn tới một bản ghi đo, hoặc bị gỡ | Kế hoạch |
| KH-6 | Giấy phép mở và quy tắc đóng góp | `LICENSE-MIT`, `LICENSE-APACHE`, `CONTRIBUTING.md` được gộp vào `main` | Một phần |

### Giai đoạn 3: vài việc làm chắc

| Mã | Việc | Xong khi | Trạng thái |
|---|---|---|---|
| KH-7 | Chọn ba việc lõi | Chủ dự án quyết định (Q-L2) | Kế hoạch |
| KH-8 | Một kịch bản kiểm cho mỗi việc lõi, chạy với mô hình mặc định | Ba kịch bản chạy lại được, có tiêu chí đạt và có kết quả ghi lại | Kế hoạch |
| KH-9 | Hành động trên máy luôn hỏi xác nhận | Không hành động nào thay đổi máy mà thiếu bước xác nhận của người dùng | Kế hoạch |

### Giai đoạn 4: người dùng đầu tiên

| Mã | Việc | Xong khi | Trạng thái |
|---|---|---|---|
| KH-10 | Quan sát ba tới năm người cài và dùng, không giúp trước khi họ kẹt | Có bảng ghi: thời gian tới câu trả lời đầu tiên, mọi chỗ kẹt, việc họ chọn làm, và sau 14 ngày họ có dùng lại không | Kế hoạch |

## 8. Không làm

- **Tự huấn luyện mô hình ngôn ngữ.** Ngoài sức của dự án.
- **Tách một mô hình ra nhiều máy.** Mỗi token phải đi qua mạng ở mỗi ranh giới, nên chậm trên mạng nhà. Lý do chi tiết ở mục "Physical limits" của [thiết kế 0038 của OxideSwarm](https://github.com/DuongNAD/OxideSwarm/blob/master/docs/design/0038-large-language-model-inference.md).
- **Bản cho macOS và Linux** trước khi xong giai đoạn 1 tới 3.
- **Gửi dữ liệu ra ngoài theo mặc định.** Mô hình ở xa chỉ được dùng khi người dùng tự bật.
- **Đo lường người dùng từ trong ứng dụng.** Số liệu về người dùng chỉ đến từ KH-10.

## 9. Quan hệ với OxideSwarm

Hôm nay không có quan hệ nào trong mã: LIVA không gọi OxideSwarm, và OxideSwarm không chạy mô hình ngôn ngữ.

OxideSwarm có hai mục ở trạng thái Kế hoạch: LLM-1 (gửi câu hỏi tới máy đang giữ mô hình) và LLM-6 (cổng tương thích OpenAI). Nếu chúng được xây, endpoint của KH-1 có thể trỏ tới cổng đó, và LIVA trở thành ứng dụng đầu tiên dùng nó. LIVA không phụ thuộc vào việc này: KH-1 chạy được với bất kỳ máy chủ tương thích OpenAI nào.

## 10. Thước đo

| Mã | Đo cái gì | Cách đo |
|---|---|---|
| T1 | Thời gian từ lúc tải bộ cài tới câu trả lời đầu tiên | Bấm giờ trong KH-10 |
| T2 | Thời gian tới token đầu trên laptop không GPU, mô hình ở máy khác | `ttft_bench` (`liva-native-core/src/bin/ttft_bench.rs`), 10 lượt, ghi cả nhỏ nhất và lớn nhất |
| T3 | Tỷ lệ đạt của ba kịch bản việc lõi | KH-8 |
| T4 | Số người dùng lại sau 14 ngày | Chủ dự án hỏi trực tiếp |

Chưa có số nào cho T1, T3 và T4. Số duy nhất đã có cho mô hình chạy tại chỗ nằm ở mục 3.

## 11. Quyết định chờ chủ dự án

| Mã | Câu hỏi | Đề xuất |
|---|---|---|
| Q-L1 | Chấp nhận câu mục đích ở mục 2 và người dùng đầu tiên? | Có |
| Q-L2 | Ba việc lõi là gì? | Hỏi đáp bằng giọng nói tiếng Việt; ghi nhớ và nhắc lại; hỏi đáp trên tệp của người dùng |
| Q-L3 | Giai đoạn 1 trước hay giai đoạn 2 trước? | Giai đoạn 1 trước, vì yêu cầu phần cứng đang loại phần lớn máy phổ thông |
| Q-L4 | Có cho phép dịch vụ ngoài làm endpoint không, hay chỉ máy của chính người dùng? | Cho phép, tắt mặc định, kèm KH-3 |
| Q-L5 | Sửa mô tả trên GitHub cho khớp câu mục đích? | Có. Mô tả hiện tại nêu "91 MCP tools" và "5-tier memory", hai con số không tài liệu nào chứng minh |

## 12. Rủi ro

| Rủi ro | Cách giảm |
|---|---|
| Một người làm hai dự án lớn | Mỗi giai đoạn có ích một mình. Không giai đoạn nào cần giai đoạn sau mới dùng được |
| Mô hình nhỏ không đủ tốt cho ba việc lõi | KH-8 đo trước khi quảng bá. H2 cho phép dùng mô hình lớn hơn ở máy khác |
| Endpoint ở xa làm lộ dữ liệu | Tắt mặc định, KH-3, và hướng dẫn KH-2 chỉ dùng mạng tin cậy vì bề mặt phục vụ chưa có xác thực |
| Kế hoạch lại bị bỏ quên như các bản trước | Đây là tài liệu kế hoạch duy nhất. Mỗi thay đổi ghi một dòng ở mục 13 |

## 13. Nhật ký

- 2026-11-11: Viết bản nháp theo yêu cầu của chủ dự án. Chưa mục nào được duyệt, chưa mục nào được xây. KH-6 ở trạng thái Một phần vì các file giấy phép đã nằm trên nhánh, chưa gộp.
