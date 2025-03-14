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
        [log-out] Összes alkalmazásból kilépés és kijelentkezés
        *[other] Alkalmazza a kiválasztott műveletet
    } most?
confirm-body = 
    A rendszer { $action ->
        [restart] újra fog indulni
        [suspend] felfüggesztésre kerül
        [shutdown] le fog állni
        [lock-screen] le fogja zárni a képernyőt
        [log-out] ki fog jelentkezni
        *[other] alkalmazni fogja a kiválasztott műveletet
    } automatikusan { $countdown } másodpercen belül.

