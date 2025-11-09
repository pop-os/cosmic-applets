power = Strøm
settings = Innstillinger...
lock-screen = Lås Skjermen
lock-screen-shortcut = Super + Escape
log-out = Logg ut
log-out-shortcut = Super + Shift + Escape
suspend = Dvalemodus
restart = Start på nytt
shutdown = Slå av
confirm = Bekreft
cancel = Avbryt
confirm-button =
    { $action ->
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] Slå av
        [log-out] { log-out }
       *[other] { confirm }
    }
confirm-title =
    { $action ->
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] { shutdown }
        [log-out] Avslutt alle programmer og logg ut
       *[other] Bruk den valgte handlingen
    } nå?
confirm-body =
    Systemet vil { $action ->
        [restart] starte på nytt
        [suspend] gå i hvilemodus
        [shutdown] slås av
        [lock-screen] låse skjermen
        [log-out] logge ut
       *[other] bruke den valgte handlingen
    } automatisk om { $countdown } sekunder.
