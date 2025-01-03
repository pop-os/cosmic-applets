power = Energie
settings = Instellingen...
lock-screen = Scherm vergrendelen
lock-screen-shortcut = Super + Escape
log-out = Afmelden
log-out-shortcut = Super + Shift + Escape
suspend = Slaapstand
restart = Opnieuw opstarten
shutdown = Afsluiten
confirm = Oké
cancel = Annuleren

<#-- Confirmation Dialog -->
confirm-button = { 
    $action -> 
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] { shutdown }
        [log-out] { log-out }
        *[other] { confirm }
    }

confirm-title = { Nu 
    $action -> 
        [restart] opnieuw opstarten?
        [suspend] in slaapstand?
        [shutdown] afsluiten?
        [log-out] alle apps sluiten en afmelden?
        *[other] de geselecteerde actie uitvoeren?
    }

confirm-body = {
    $action -> 
        [restart] De computer start
        [suspend] De computer gaat
        [shutdown] De computer wordt
        [lock-screen] Het vergrendelscherm wordt
        [log-out] De gebruiker wordt
        *[other] De geselecteerde actie wordt
    } in { $countdown } seconden automatisch {
    $action ->
        [restart] opnieuw op.
        [suspend] in slaapstand.
        [shutdown] afgesloten.
        [lock-screen] geactiveerd.
        [log-out] afgemeld.
        *[other] uitgevoerd.
    }






