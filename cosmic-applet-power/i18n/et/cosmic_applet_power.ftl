power = Toide
settings = Seadistused...
lock-screen = Lukustusvaade
lock-screen-shortcut = Super + Escape
log-out = Logi välja
log-out-shortcut = Super + Shift + Escape
suspend = Unne
restart = Käivita arvuti uuesti
shutdown = Lülita välja
confirm = Kinnita
cancel = Katkesta
confirm-button =
    { $action ->
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] Lülita välja
        [log-out] { log-out }
       *[other] { confirm }
    }
