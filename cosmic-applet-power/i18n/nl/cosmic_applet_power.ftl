power = Energie
settings = Instellingen...
lock-screen = Scherm vergrendelen
lock-screen-shortcut = Super + Esc
log-out = Afmelden
log-out-shortcut = Super + Shift + Esc
suspend = Slaapstand
restart = Opnieuw opstarten
shutdown = Afsluiten
confirm = Bevestigen
cancel = Annuleren
confirm-button = {
    $action -> 
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] { shutdown }
        [log-out] { log-out }
        *[other] Uitvoeren
}
confirm-title = Nu
    { $action -> 
        [restart] opnieuw opstarten?
        [suspend] in slaapstand gaan?
        [shutdown] afsluiten?
        [log-out] alle apps sluiten en afmelden?
        *[other] de geselecteerde actie uitvoeren?
    } 
confirm-body = 
    De { $action ->
        [restart] computer wordt in {$countdown} seconden automatisch opnieuw opgestart.
        [suspend] computer wordt in {$countdown} seconden automatisch in slaapstand gezet.
        [shutdown] computer wordt in {$countdown} seconden automatisch afgesloten.
        [lock-screen] schermvergrendeling wordt in {$countdown} seconden automatisch geactiveerd.
        [log-out] gebruiker wordt in {$countdown} seconden automatisch afgemeld.
        *[other] geselecteerde actie wordt in {$countdown} seconden automatisch uitgevoerd.
    }
