# VKeyEngine

Bộ gõ tiếng Việt cho macOS và Windows, viết bằng Rust. Một engine dùng chung, giao diện native cho từng hệ điều hành theo cùng một phong cách đen trắng.

- Kiểu gõ: Telex, VNI, Simple Telex 1 / 2
- Bảng mã: Unicode dựng sẵn, TCVN3 (ABC), VNI Windows, Unicode tổ hợp, Vietnamese Locale CP 1258
- Kiểm tra chính tả, tự khôi phục từ sai, đặt dấu kiểu mới, gõ nhanh phụ âm
- Giữ nguyên từ tiếng Anh khi gõ Telex (boss, good, search…), bấm Esc để trả lại đúng các phím vừa gõ
- Hiện V / E cạnh con trỏ khi chuyển chế độ
- Không gõ tiếng Việt trong các ứng dụng chọn trước (Terminal, IDE, game…)
- Gõ tắt (nhập / xuất định dạng UniKey), tự viết hoa theo từ gõ tắt
- Tự nhớ chế độ gõ và bảng mã theo từng ứng dụng
- Công cụ chuyển mã clipboard (văn bản và HTML); phím tắt chuyển mã ngay văn bản đang chọn
- Tự kiểm tra và cài bản mới từ GitHub Releases (có thể tắt)
- Trang "Hoạt động": RAM, CPU, thời gian xử lý mỗi phím

## Build

```sh
cargo test                   # test engine
./build-macos.sh             # macOS -> target/VKeyEngine.app   (--debug, --universal)
cargo build --release        # Windows -> target\release\VKeyEngine.exe
```

macOS cần quyền Trợ năng (Accessibility). `build-macos.sh` ký bằng chứng chỉ Apple Development nếu có (đặt `SIGN_ID` để chọn chứng chỉ khác), để quyền không mất sau mỗi lần build; không có chứng chỉ thì ký ad-hoc. Màu nhấn đen trắng cần `actool` của Xcode.

### Ký bản macOS

Bản phát hành được ký bằng một chứng chỉ tự ký cố định ("VKeyEngine Code Signing"), để macOS nhận ra vẫn là app cũ sau mỗi lần cập nhật và giữ quyền Trợ năng. CI đọc chứng chỉ từ hai secret của repo:

- `MACOS_CERT_P12`: file `.p12` mã hoá base64
- `MACOS_CERT_PASSWORD`: mật khẩu của file `.p12`

Thiếu secret thì CI vẫn build (ký ad-hoc), riêng khi push tag thì báo lỗi. Đổi chứng chỉ sẽ làm người dùng phải cấp lại quyền một lần. Giữ kín khoá riêng: ai có nó ký được app mà macOS coi là VKeyEngine, kể cả quyền Trợ năng đã cấp.

CI (`.github/workflows/vkeyengine.yml`) chạy test và build cả hai bản trên GitHub Actions. Push một tag `vX.Y.Z` (khớp `version` trong `Cargo.toml`) để CI tạo Release; app sẽ tìm thấy bản đó khi kiểm tra cập nhật.

## Cấu trúc

| Thư mục | Nội dung |
|---|---|
| `src/engine.rs`, `tables.rs`, `macros.rs`, `convert.rs`, `smart_switch.rs` | Engine, độc lập nền tảng |
| `src/mac/`, `macos/` | macOS: event tap, menu bar, cửa sổ AppKit; icon, màu nhấn |
| `src/win/`, `windows/` | Windows: hook bàn phím (luồng riêng), khay hệ thống, cửa sổ tự vẽ; icon, manifest |
| `tests/` | Test engine |

Người dùng chuyển từ OpenKey: cấu hình và gõ tắt được chép sang ở lần chạy đầu.

## Giấy phép

GPL v3 (xem `LICENSE`).

`src/english.txt` (các từ tiếng Anh thông dụng mà Telex làm biến dạng) được lọc từ danh sách 50 000 từ của [FrequencyWords](https://github.com/hermitdave/FrequencyWords) © Hermit Dave, giấy phép [CC BY-SA 4.0](https://creativecommons.org/licenses/by-sa/4.0/). VKeyEngine là bản phát triển tiếp từ [OpenKey](https://github.com/tuyenvm/OpenKey) © 2019 Mai Vũ Tuyên, cũng theo GPL v3.
