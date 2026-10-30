log-out-shortcut = Super + Shift + Phím cách
lock-screen-shortcut = Super + Phím cách
confirm-button =
    { $action ->
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] Tắt nguồn
        [log-out] { log-out }
       *[other] { confirm }
    }
confirm-body =
    Hệ thống sẽ { $action ->
        [restart] khởi động lại
        [suspend] tạm ngưng
        [shutdown] tắt nguồn
        [lock-screen] khóa màn hình
        [log-out] đăng xuất
       *[other] áp dụng hành động đã chọn
    } tự động trong { $countdown } giây.
suspend = Tạm ngưng
settings = Cài đặt...
confirm-title =
    { $action ->
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] { shutdown }
        [log-out] Thoát tất cả ứng dụng và đăng xuất
       *[other] Thực thi lựa chọn
    } bây giờ?
