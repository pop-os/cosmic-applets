power = Napájení
settings = Nastavení...
lock-screen = Zamknout obrazovku
lock-screen-shortcut = Super + Escape
log-out = Odhlásit se
log-out-shortcut = Super + Shift + Escape
suspend = Uspat
restart = Restartovat
shutdown = Vypnout
confirm = Potvrdit
cancel = Zrušit
confirm-button = {
    $action -> 
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] Vypnout
        [log-out] { log-out }
        *[other] { confirm}
}
confirm-title = 
    { $action -> 
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] { shutdown }
        [log-out] Ukončit všechny aplikace a odhlásit se
        *[other] Použít vybranou akci
    } nyní?
confirm-body = 
    Systém { $action ->
        [restart] se restartuje
        [suspend] se uspí
        [shutdown] se vypne
        [lock-screen] se uzamkne
        [log-out] odhlásí uživatele
        *[other] vykoná vybranou akci
    } automaticky za { $countdown ->
        [one] { $countdown } sekundu.
        [few] { $countdown } sekundy.
        [many] { $countdown } sekund.
        *[other] { $countdown } sekund.
    }
