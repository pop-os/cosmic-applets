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
confirm-button = {
    $action -> 
        [restart] Újraindítás
        [suspend] Felfüggesztés
        [shutdown] Leállítás
        [log-out] Kijelentkezés
        *[other] Megerősítés
}
confirm-title = 
    { $action -> 
        [restart] Újraindítás
        [suspend] Felfüggesztés
        [shutdown] Leállítás
        [log-out] Minden alkalmazás bezárása és kijelentkezés
        *[other] Végrehajtja a kiválasztott műveletet
    } most?
confirm-body = 
    A rendszer automatikusan { $action ->
        [restart] újra fog indulni
        [suspend] felfüggesztésre kerül
        [shutdown] le fog állni
        [lock-screen] zárolni fogja a képernyőt
        [log-out] kijelentkezik
        *[other] végrehajtja a kiválasztott műveletet
    } { $countdown } másodperc múlva.
