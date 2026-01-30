power = Daya
settings = Pengaturan...
lock-screen = Layar Kunci
lock-screen-shortcut = Super + Escape
log-out = Keluar
log-out-shortcut = Super + Shift + Escape
suspend = Hentikan
restart = Mulai ulang
shutdown = Matikan
confirm = Konfirmasi
cancel = Batalkan
confirm-button =
    { $action ->
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] Matikan daya
        [log-out] { log-out }
       *[other] { confirm }
    }
confirm-title =
    { $action ->
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] { shutdown }
        [log-out] Keluar semua aplikasi dan keluar
       *[other] Terapkan tindakan yang dipilih
    } sekarang?
confirm-body =
    Sistem akan { $action ->
        [restart] memulai ulang
        [suspend] menghentikan
        [shutdown] mematikan
        [lock-screen] mengunci layar
        [log-out] keluar
       *[other] menerapkan tindakan yang dipilih
    } secara otomatis dalam { $countdown } detik.
