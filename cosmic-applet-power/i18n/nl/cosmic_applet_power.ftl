power = Stroom
settings = Instellingen...
lock-screen = Vergrendel Scherm
lock-screen-shortcut = Super + Escape
log-out = Uit loggen
log-out-shortcut = Ctrl + Alt + Delete
suspend = Slaapstand
restart = Herstarten
shutdown = Afsluiten
confirm = Bevestigen
cancel = Annuleren
confirm-body = 
    { $action ->
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] { shutdown }
        [lock-screen] Het scherm vergrendelen
        [log-out] Uit aan het loggen
        *[other] de geselecteerde actie
    } gaat verder in { $countdown } seconden.

