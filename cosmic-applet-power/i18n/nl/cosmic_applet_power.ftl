power = Stroom
settings = Instellingen...
lock-screen = Vergrendelscherm
lock-screen-shortcut = Super + Escape
log-out = Afmelden
log-out-shortcut = Super + Shift + Escape
suspend = Slaapstand
restart = Opnieuw opstarten
shutdown = Afsluiten
confirm = Bevestigen
cancel = Annuleren

confirm-button =
    { $action -> 
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
        [log-out] Alle applicaties sluiten en afmelden
        *[other] De geselecteerde actie
    } nu uitvoeren?

confirm-body = 
    { $action ->
        [restart] De computer start in { $countdown } seconden automatisch opnieuw op.
        [suspend] De computer gaat in { $countdown } seconden automatisch in slaapstand.
        [shutdown] De computer wordt in { $countdown } seconden automatisch afgesloten.
        [lock-screen] Het vergrendelscherm wordt in { $countdown } seconden automatisch geactiveerd.
        [log-out] U wordt in { $countdown } seconden automatisch afgemeld.
        *[other] De geselecteerde actie wordt in { $countdown } seconden automatisch uitgevoerd.
    }

