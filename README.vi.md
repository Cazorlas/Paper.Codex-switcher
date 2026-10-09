# paper-codex-switch

[English](README.md) · **Tiếng Việt**

**Công cụ đổi nhiều tài khoản cho [OpenAI Codex CLI](https://github.com/openai/codex), tự động đổi tài khoản trước khi chạm giới hạn sử dụng.**

Lưu nhiều tài khoản Codex, xem hạn mức 5 giờ và hạn mức tuần của tất cả trong một bảng, đổi tài khoản bằng một lệnh, và để `auto` tự chuyển sang tài khoản còn nhiều hạn mức trước khi tài khoản đang dùng hết.

![paper-codex-switch list](docs/list.png)

> Lấy cảm hứng từ [codex-switch](https://github.com/xjoker/codex-switch) và [claude-swap](https://github.com/realiti4/claude-swap). Ảnh trên dùng tài khoản giả.

## Tính năng

- Lưu, nhập, đổi tên và xóa (có thể khôi phục) tài khoản; đổi theo tên, theo số thứ tự, hoặc tự chọn tài khoản tốt nhất.
- Bảng hạn mức (lệnh `list` và giao diện `tui`) cho cửa sổ 5h và 7d, hiện **phần trăm đã dùng** (tăng dần khi bạn làm việc; xanh dưới 70%, vàng từ 70%, đỏ từ 90%), gói đang dùng, thẻ reset, và ngày gói hết hạn (cột `Plan until`, màu vàng khi còn 7 ngày hoặc ít hơn).
- **`auto`**: vòng lặp tự đổi tài khoản khi tài khoản đang dùng gần hết hạn mức (để cửa sổ mở, thu nhỏ lại).
- **`launch --auto-swap`**: chạy Codex và, khi chạm ngưỡng, khởi động lại phiên bằng tài khoản khác với `codex resume --last`.
- **`status --short`**: một dòng cho thanh trạng thái hoặc dấu nhắc lệnh, ví dụ `paperengineer05 5h 22% 7d 47%`.
- Nhà cung cấp API tùy chỉnh (beta), proxy, xuất JSON.
- Windows, macOS và Linux.

## Yêu cầu

- [Codex CLI](https://github.com/openai/codex) bản 0.159.2 trở lên có trong `PATH` (`paper-codex-switch doctor` sẽ kiểm tra).
- Codex phải dùng kho thông tin đăng nhập dạng tệp. Thêm vào `~/.codex/config.toml`:

  ```toml
  cli_auth_credentials_store = "file"
  ```

## Cài đặt

```bash
npm i -g paper-codex-switch
```

Lệnh này tải chương trình dựng sẵn cho máy bạn (Windows x64, macOS x64/arm64, Linux x64) từ [GitHub Releases](https://github.com/Cazorlas/Paper.Codex-switcher/releases). Chỉ cần có Node 18 trở lên.

Hoặc tự dựng từ mã nguồn (Rust 1.88+):

```bash
cargo install --git https://github.com/Cazorlas/Paper.Codex-switcher
```

Dữ liệu nằm ở `~/.paper-codex-switch` (đổi bằng biến môi trường `PAPER_CODEX_SWITCH_HOME`). Cập nhật bằng `paper-codex-switch self-update`.

## Thiết lập lần đầu

1. Cài Codex CLI (0.159.2 trở lên) và bật kho đăng nhập dạng tệp (xem phần Yêu cầu).
2. Cài paper-codex-switch rồi kiểm tra bằng:
   ```bash
   paper-codex-switch doctor
   ```
3. Thêm tài khoản, mỗi tài khoản một lần `login` (trình duyệt sẽ mở; dùng `--device` trên máy không có giao diện):
   ```bash
   paper-codex-switch login personal
   paper-codex-switch login work
   ```
   Đã đăng nhập Codex bằng một tài khoản khác rồi? Chỉ cần chạy `paper-codex-switch list`: công cụ sẽ phát hiện tài khoản trong `~/.codex/auth.json` và hỏi bạn có muốn lưu lại không.
4. Kiểm tra: `paper-codex-switch list`.

## Dùng hằng ngày

```bash
paper-codex-switch list           # bảng hạn mức, có đánh số (cửa sổ 5h và 7d)
paper-codex-switch use            # đổi sang tài khoản tốt nhất
paper-codex-switch use 2          # đổi sang tài khoản số 2 trong `list`
paper-codex-switch use work       # ...hoặc theo tên
paper-codex-switch launch         # chạy Codex bằng tài khoản tốt nhất
paper-codex-switch tui            # giao diện tương tác (cửa sổ hẹp thì mỗi tài khoản là một khối xếp chồng)
paper-codex-switch auto           # tự đổi khi gần chạm giới hạn (xem bên dưới)
paper-codex-switch status --short # một dòng cho thanh trạng thái
```

`use <số>` dùng số thứ tự hiện trong `list` (xếp theo tên); nếu có tài khoản đặt tên đúng là `2` thì tên được ưu tiên hơn vị trí 2.

Ngày hết hạn gói lấy từ token đăng nhập của tài khoản, nên chỉ mới như lần làm mới token gần nhất. `lapsed?` nghĩa là ngày ghi trong token đã qua (gói đã hết, hoặc đã gia hạn mà token chưa làm mới; `list --force` hoặc `login <tên>` sẽ làm mới). Bản `--json list` có trường `account.subscription_until` (giây Unix).

## Quản lý tài khoản

| Việc cần làm | Lệnh |
|---|---|
| Thêm tài khoản | `paper-codex-switch login [tên]` |
| Đăng nhập lại tài khoản hết hạn | `paper-codex-switch login <tên đã có>` |
| Nhập tệp `auth.json` hoặc một thư mục chứa chúng | `paper-codex-switch import <đường dẫn> [tên]` |
| Đổi tên | `paper-codex-switch rename <cũ> <mới>` |
| Xóa (được lưu lại để khôi phục; không xóa được tài khoản đang dùng) | `paper-codex-switch delete <tên> [--yes]` |
| Xem tài khoản đã xóa / khôi phục | `paper-codex-switch restore` / `paper-codex-switch restore <tên> [--as <tên mới>]` |
| Làm mới hạn mức ngay, bỏ qua bộ nhớ đệm | `paper-codex-switch list --force` |
| Bắt đầu đồng hồ 5 giờ của tài khoản mới | `paper-codex-switch warmup [tên]` |
| Dùng thẻ reset cho một tài khoản | `paper-codex-switch reset-card <tên>` |
| Mở thư mục dữ liệu | `paper-codex-switch open` |
| Kiểm tra phiên bản Codex và cấu hình | `paper-codex-switch doctor` |

Cờ chung: `--json` / `--json-pretty` (xuất cho máy đọc), `--proxy <url>`, `--color always|never`, `--debug`. Cấu hình nằm ở `~/.paper-codex-switch/config.toml` (cũng sửa được trong tab Settings của `tui`).

Muốn bỏ bớt tài khoản không dùng: `list`, rồi `delete <tên>`. Xóa không phải là mất hẳn: `restore` liệt kê những gì đã xóa và `restore <tên>` đưa bản mới nhất về (dùng `--as` nếu tên đã bị chiếm). Bản lưu nằm ở `~/.paper-codex-switch/deleted-profiles`. `auto` chỉ đổi giữa các tài khoản đang lưu, nên xóa một tài khoản cũng là đưa nó ra khỏi vòng đổi.

### Cập nhật / gỡ cài đặt

```bash
paper-codex-switch self-update --check   # có bản mới không?
paper-codex-switch self-update           # cập nhật (cài bằng npm)
paper-codex-switch uninstall             # gỡ cài đặt
```

Mỗi ngày một lần, lệnh kiểm tra ngầm bản mới và in một dòng nhắc khi có; nó không bao giờ tự cài. `self-update` đóng `auto` đang chạy (Windows khóa tệp `.exe` đang chạy) rồi chạy `npm i -g paper-codex-switch@latest`; hãy chạy lại `auto` sau đó. Cài bằng cargo thì chạy lại `cargo install --git https://github.com/Cazorlas/Paper.Codex-switcher`.

### Gỡ cài đặt

```bash
paper-codex-switch uninstall              # đóng auto, gỡ gói npm,
                                          # rồi hỏi có xóa tài khoản và cài đặt đã lưu không [y/N]
paper-codex-switch uninstall --purge      # ...xóa luôn tài khoản và cài đặt, không hỏi
paper-codex-switch uninstall --keep-data  # ...giữ lại, không hỏi
```

Lệnh này chạy được cả khi chương trình bị chặn hoặc hỏng vì nó chạy từ trình khởi chạy npm, và khi chạy không có cửa sổ tương tác thì mặc định giữ dữ liệu. Cài bằng cargo: `cargo uninstall paper-codex-switch`, rồi xóa `~/.paper-codex-switch` nếu muốn xóa cả dữ liệu.

### Windows Defender / SmartScreen

Tệp `.exe` trên Windows chưa được ký số, nên Defender đôi khi báo bằng một kết luận máy học chung chung (ví dụ `Trojan:Win32/Bearfoos.A!ml`) và cách ly nó; khi đó npm báo `spawnSync ... UNKNOWN`. Đây là báo nhầm. Hãy đối chiếu tệp với mã SHA256 của đúng phiên bản đó trên trang [Releases](https://github.com/Cazorlas/Paper.Codex-switcher/releases), rồi vào Windows Security → Protection history chọn **Allow on device**, và cài lại bằng `npm i -g paper-codex-switch@latest`. Có thể báo nhầm cho Microsoft tại https://www.microsoft.com/wdsi/filesubmission.

## Tự động đổi tài khoản

```bash
paper-codex-switch auto                        # vòng lặp ở cửa sổ này, kiểm tra mỗi 60 giây
paper-codex-switch auto --threshold 95         # đổi muộn hơn (mặc định 90)
paper-codex-switch auto --threshold 95 --prefer-expiring # dùng tài khoản có gói sắp hết hạn trước
paper-codex-switch auto --dry-run              # chỉ báo sẽ làm gì, không đổi thật
paper-codex-switch auto --toast                # thêm thông báo Windows khi đổi hoặc hết tài khoản
paper-codex-switch --json auto --once          # kiểm tra một lần, cho cron / Task Scheduler
```

| Tùy chọn | Mặc định | Ý nghĩa |
|---|---|---|
| `--threshold` | 90 | đổi khi cửa sổ 5h hoặc 7d dùng tới mức này (%) |
| `--margin` | 10 | tài khoản đích phải thấp hơn tài khoản hiện tại ít nhất ngần này điểm |
| `--prefer-expiring [DAYS]` | tắt; 7 khi không ghi DAYS | ưu tiên tài khoản đủ điều kiện có gói hết hạn sớm nhất trong DAYS ngày, kể cả dưới ngưỡng; khi dưới ngưỡng, gói đích phải hết hạn trước gói hiện tại và mức dùng không quá ngưỡng trừ margin |
| `--interval` | 60 | số giây giữa hai lần kiểm tra |
| `--cooldown` | 300 | số giây tối thiểu giữa hai lần đổi |
| `--once` | tắt | kiểm tra một lần rồi thoát |
| `--dry-run` | tắt | chỉ báo cáo |
| `--toast` | tắt | thêm thông báo Windows (chuông terminal luôn được gửi) |

Cách nó quyết định:

1. Mỗi `--interval` nó chỉ kiểm tra tài khoản đang dùng. Dưới ngưỡng thì không làm gì thêm (không gọi mạng cho các tài khoản khác), trừ khi `--prefer-expiring` tìm thấy gói hết hạn sớm hơn trong các tệp tài khoản cục bộ.
2. Từ ngưỡng trở lên, nó làm mới tất cả tài khoản, xếp hạng bằng cùng thuật toán chấm điểm của `use`, và chọn tài khoản tốt nhất đủ điều kiện, dưới ngưỡng và thấp hơn `--margin` điểm.
3. Việc đổi có kiểm tra tài khoản đang dùng chưa bị tiến trình khác đổi, nên không ghi đè thay đổi của nơi khác. Daemon app-server của Codex đang chạy sẽ được khởi động lại sau đó, giống sau `use`.
4. Nếu mọi tài khoản đều cạn, nó giãn nhịp kiểm tra còn 10 phút một lần. Lỗi khi kiểm tra hạn mức thì giữ tài khoản hiện tại và thử lại.
5. Khi đổi hoặc khi hết tài khoản, nó phát chuông terminal (làm nhấp nháy cửa sổ đang thu nhỏ trên thanh tác vụ); thêm `--toast` để có thông báo Windows.

Mã thoát của `--once`: `0` đã đổi, `1` lỗi, `2` không cần làm gì, `3` bị kẹt (không có tài khoản đích). Với `--json` mỗi sự kiện là một dòng JSON.

Các phiên Codex đang chạy giữ thông tin đăng nhập lúc chúng khởi động; việc đổi chỉ áp dụng cho phiên mới. Để chuyển cả phiên đang chạy, dùng:

```bash
paper-codex-switch launch --auto-swap [--swap-threshold 85] [-- tham số của codex]
```

Khi chạm ngưỡng, nó dừng Codex (cả các lệnh Codex đang chạy) và chạy lại bằng tài khoản tốt hơn với `codex resume --last`. Lượt đang chạy dở bị mất, cuộc trò chuyện được nối lại.

### Giữ cho nó chạy

`auto` chạy ngay trong cửa sổ terminal nơi bạn khởi động. Cứ để cửa sổ đó mở và thu nhỏ (trợ giúp của TUI, phím `h`, cũng liệt kê các lệnh này); nó âm thầm kiểm tra và chỉ in một dòng khi có việc hoặc có lỗi. Muốn dừng thì nhấn Ctrl+C hoặc đóng cửa sổ. Chạy lại là `paper-codex-switch auto`.

Cố ý không có dịch vụ chạy ngầm ẩn: không có gì tự đăng ký chạy cùng Windows, nên không có gì phải cài thêm hay gỡ bỏ. Nếu muốn chạy không cần người trông, hãy gọi một lần kiểm tra từ trình lập lịch (cron, Windows Task Scheduler):

```bash
paper-codex-switch auto --once --json
```

## Bảo mật

Công cụ này quản lý các tệp đăng nhập cục bộ. Đừng bao giờ đăng công khai profile, `auth.json`, token hay đầu ra gỡ lỗi chưa che thông tin.

## Phát triển

```bash
cargo build && cargo test
```

Phát hành: đẩy tag `v<phiên bản>`; `.github/workflows/release.yml` dựng bốn bản chương trình, đính vào release rồi tự đăng lên npm.

## Giấy phép

MIT, © Cazorlas, xem `LICENSE`. Một phần mã được chỉnh từ [codex-switch](https://github.com/xjoker/codex-switch) (MIT); thông báo bản quyền của họ nằm trong `THIRD_PARTY_NOTICES.md`.
