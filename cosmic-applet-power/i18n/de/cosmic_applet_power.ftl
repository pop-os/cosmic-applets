power = Energie
settings = Einstellungen...
lock-screen = Sperrbildschirm
lock-screen-shortcut = Super + Esc
log-out = Abmelden
log-out-shortcut = Super + Umschalt + Escape
suspend = Bereitschaft
restart = Neustart
shutdown = Herunterfahren
confirm = Bestätigen
cancel = Abbrechen
confirm-button =
    { $action ->
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] Ausschalten
        [log-out] { log-out }
       *[other] { confirm }
    }
confirm-title =
    Jetzt { $action ->
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] { shutdown }
        [log-out] alle Anwendungen beenden und abmelden
       *[other] die ausgewählte Aktion anwenden
    }?
confirm-body =
    Das System wird in { $countdown } Sekunden automatisch { $action ->
        [restart] neu gestartet
        [suspend] in Bereitschaft versetzt
        [shutdown] ausgeschaltet
        [lock-screen] gesperrt
        [log-out] abgemeldet
       *[other] die ausgewählte Aktion anwenden
    }.
