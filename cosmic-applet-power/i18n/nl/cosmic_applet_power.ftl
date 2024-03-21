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
confirm-question = 
    Weet je het zeker? { $action ->
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] { shutdown }
        [lock-screen] Het scherm vergrendelen
        [log-out] Uit aan het loggen
        *[other] de geselecteerde actie
    } gaat verder in { $countdown ->
    	[1] 1 seconde.
    	*[other] {$countdown} seconden.
    }

