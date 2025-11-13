power = Főkapcsoló
settings = Beállítások...
lock-screen = Képernyő zárolása
lock-screen-shortcut = Super + Escape
log-out = Kijelentkezés
log-out-shortcut = Super + Shift + Escape
suspend = Felfüggesztés
restart = Újraindítás
shutdown = Leállítás
confirm = Megerősítés
cancel = Mégse
confirm-button =
    { $action ->
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] Leállítás
        [log-out] { log-out }
       *[other] { confirm }
    }
confirm-title =
    { $action ->
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] { shutdown }
        [log-out] Minden alkalmazás bezárása és kijelentkezés
       *[other] Alkalmazzuk a kiválasztott műveletet
    } most?
confirm-body =
    A rendszer automatikusan { $action ->
        [restart] újra fog indulni
        [suspend] felfüggesztésre kerül
        [shutdown] leáll
        [lock-screen] zárolni fogja a képernyőt
        [log-out] kijelentkezik
       *[other] alkalmazni fogja a kiválasztott műveletet
    } { $countdown } másodperc múlva.
