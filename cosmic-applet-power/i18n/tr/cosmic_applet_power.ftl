power = Güç
settings = Ayarlar...
lock-screen = Ekranı kilitle
lock-screen-shortcut = Super + Escape
log-out = Oturumu Kapat
suspend = Askıya Al
restart = Yeniden Başlat
shutdown = Kapat
confirm = Onayla
cancel = Vazgeç
confirm-button =
    { $action ->
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] Kapat
        [log-out] { log-out }
       *[other] { confirm }
    }
confirm-title =
    { $action ->
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] { shutdown }
        [log-out] Tüm uygulamalardan çık ve oturumu kapat
       *[other] Seçilen eylemi uygula
    } işlemi uygulansın mı?
confirm-body =
    Sistem { $countdown } saniye içinde { $action ->
        [restart] yeniden başlatılacak.
        [suspend] askıya alınacak.
        [shutdown] kapatılacak.
        [lock-screen] ekranı kitleyecek.
        [log-out] çıkış yapacak.
       *[other] seçilen eylemi uygulayacak.
    }
log-out-shortcut = Süper + Shift + Escape
