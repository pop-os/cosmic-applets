power = Strøm
confirm-body =
    Systemet vil automatisk { $action ->
        [restart] genstarte
        [suspend] slumre
        [shutdown] slukke
        [lock-screen] låse skærmen
        [log-out] logge ud
       *[other] anvende den valgte handling
    } om { $countdown } sekunder.
lock-screen = Låseskærm
shutdown = Nedlukning
log-out = Log Ud
restart = Genstart
log-out-shortcut = Super + Skift + Escape
cancel = Afbryd
suspend = Slumre
confirm = Bekræft
settings = Indstillinger...
lock-screen-shortcut = Super + Escape
confirm-title =
    { $action ->
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] { shutdown }
        [log-out] Forlad alle applikationer og log ud
       *[other] Anvend den valgte handling
    } nu?
