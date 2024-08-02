power = Napájanie
settings = Nastavenia...
lock-screen = Zamknutá obrazovka
lock-screen-shortcut = Super + Escape
log-out = Odhlásiť sa
log-out-shortcut = Ctrl + Alt + Delete
suspend = Uspať
restart = Reštartovať
shutdown = Vypnúť
confirm = Potvrdiť
cancel = Zrušiť
confirm-button = {
    $action -> 
        [restart] { restart }
        [suspend] { suspend}
        [shutdown] Vypnúť
        [log-out] { log-out }
        *[other] { confirm}
}
confirm-title = 
    { $action -> 
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] { shutdown }
        [log-out] Ukončiť všetky spustené aplikácie a odhlásiť sa
        *[other] Vykonať vybranú operáciu
    } teraz?
confirm-body = 
    Systém { $action ->
        [restart] sa reštartuje
        [suspend] sa uspí
        [shutdown] sa vypne
        [lock-screen] sa zamkne
        [log-out] odhlási prihláseného používateľa
        *[other] vykoná vybranú operáciu
    } automaticky za { $countdown } sekúnd.

