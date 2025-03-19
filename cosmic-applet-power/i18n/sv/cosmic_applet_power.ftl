power = Ström
settings = Inställningar...
lock-screen = Lås skärm
lock-screen-shortcut = Super + Escape
log-out = Logga ut
log-out-shortcut = Super + Shift + Escape
suspend = Vänteläge
restart = Starta om
shutdown = Stäng av
confirm = Bekräfta
cancel = Avbryt
confirm-button = {
    $action -> 
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] { shutdown }
        [log-out] { log-out }
        *[other] { confirm }
}
confirm-title = 
    { $action -> 
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] { shutdown }
        [log-out] Avsluta alla applikationer och logga ut
        *[other] Tillämpa vald åtgärd
    } nu?
confirm-body = 
    Systemet kommer att { $action ->
        [restart] starta om
        [suspend] försättas i viloläge
        [shutdown] stängas av
        [lock-screen] låsa skärmen
        [log-out] logga ut
        *[other] tillämpa vald åtgärd
    } automatiskt om { $countdown } sekunder.
