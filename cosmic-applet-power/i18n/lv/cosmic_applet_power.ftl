power = Barošana
settings = Iestatījumi...
lock-screen = Bloķēt ekrānu
lock-screen-shortcut = Super + Escape
log-out = Izrakstīties
log-out-shortcut = Super + Shift + Escape
suspend = Iesnaudināt
restart = Pārstartēt
shutdown = Izslēgt
confirm = Apstiprināt
cancel = Atcelt
confirm-button =
    { $action ->
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] Izslēgt
        [log-out] { log-out }
       *[other] { confirm }
    }
confirm-title =
    { $action ->
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] { shutdown }
        [log-out] Aizvērt visas lietotnes un izrakstīties
       *[other] Veikt atlasīto darbību
    } tagad?
confirm-body =
    Sistēma { $action ->
        [restart] tiks pārstartēta
        [suspend] tiks iesnaudināta
        [shutdown] tiks izslēgta
        [lock-screen] bloķēs ekrānu
        [log-out] izrakstīs lietotāju
       *[other] veiks atlasīto darbību
    } automātiski pēc { $countdown } s.
