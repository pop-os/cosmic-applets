power = Napájanie
settings = Nastavenia...
lock-screen = Uzamknúť obrazovku
lock-screen-shortcut = Super + Escape
log-out = Odhlásiť sa
log-out-shortcut = Super + Shift + Escape
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
        [log-out] Ukončiť všetky aplikácie a odhlásiť sa
        *[other] Použiť vybranú akciu
    } teraz?
confirm-body = 
    Systém { $action ->
        [restart] sa reštartuje
        [suspend] sa uspí
        [shutdown] sa vypne
        [lock-screen] sa uzamkne
        [log-out] odhlási prihláseného používateľa
        *[other] vykoná vybranú operáciu
    } automaticky za { $countdown } sekúnd.

