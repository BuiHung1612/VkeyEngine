# VKeyEngine

Bộ gõ tiếng Việt cho macOS và Windows, viết bằng Rust. Một engine dùng chung, giao diện native cho từng hệ điều hành theo cùng một phong cách đen trắng.

- Kiểu gõ: Telex, VNI, Simple Telex 1 / 2
- Bảng mã: Unicode dựng sẵn, TCVN3 (ABC), VNI Windows, Unicode tổ hợp, Vietnamese Locale CP 1258
- Kiểm tra chính tả, tự khôi phục từ sai, đặt dấu kiểu mới, gõ nhanh phụ âm
- Gõ tắt (nhập / xuất định dạng UniKey), tự viết hoa theo từ gõ tắt
- Tự nhớ chế độ gõ và bảng mã theo từng ứng dụng
- Công cụ chuyển mã clipboard (văn bản và HTML), có phím tắt

## Build

```sh
cargo test                   # test engine
./build-macos.sh             # macOS -> target/VKeyEngine.app   (--debug, --universal)
cargo build --release        # Windows -> target\release\VKeyEngine.exe
```

macOS cần quyền Trợ năng (Accessibility). `build-macos.sh` ký bằng chứng chỉ Apple Development nếu có (đặt `SIGN_ID` để chọn chứng chỉ khác), để quyền không mất sau mỗi lần build; không có chứng chỉ thì ký ad-hoc. Màu nhấn đen trắng cần `actool` của Xcode.

CI (`.github/workflows/vkeyengine.yml`) chạy test và build cả hai bản trên GitHub Actions, kèm file tải về.

## Cấu trúc

| Thư mục | Nội dung |
|---|---|
| `src/engine.rs`, `tables.rs`, `macros.rs`, `convert.rs`, `smart_switch.rs` | Engine, độc lập nền tảng |
| `src/mac/`, `macos/` | macOS: event tap, menu bar, cửa sổ AppKit; icon, màu nhấn |
| `src/win/`, `windows/` | Windows: hook bàn phím (luồng riêng), khay hệ thống, cửa sổ tự vẽ; icon, manifest |
| `tests/` | Test engine |

Người dùng chuyển từ OpenKey: cấu hình và gõ tắt được chép sang ở lần chạy đầu.

## Giấy phép

GPL v3 (xem `LICENSE`). VKeyEngine là bản phát triển tiếp từ [OpenKey](https://github.com/tuyenvm/OpenKey) © 2019 Mai Vũ Tuyên, cũng theo GPL v3.
