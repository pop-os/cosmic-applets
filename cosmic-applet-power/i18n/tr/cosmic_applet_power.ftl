power = Güç
settings = Ayarlar...
lock-screen = Ekranı kilitle
lock-screen-shortcut = Super + Escape
log-out = Oturumu kapat
log-out-shortcut = Ctrl + Alt + Delete
suspend = Askıya al
restart = Yeniden başlat
shutdown = Kapat
confirm = Onayla
cancel = İptal
confirm-button = {
    $action -> 
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] { shutdown }
        [log-out] { log-out }
        *[other] { confirm}
}
confirm-title = 
    { $action -> 
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] { shutdown }
        [log-out] Tüm uygulamalardan çık ve oturumu kapat
        *[other] Seçilen eylemi uygula
    } now?
confirm-body = 
    Sistem { $countdown } saniye içinde { $action ->
        [restart] yeniden başlatılacaktır.
        [suspend] askıya alınacaktır.
        [shutdown] kapatılacaktır.
        [lock-screen] ekranı kitleyecektir.
        [log-out] çıkış yapacaktır.
        *[other] seçilen eylemi uygulayacaktır.
    }
