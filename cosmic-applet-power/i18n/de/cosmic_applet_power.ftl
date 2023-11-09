power = Energie
settings = Einstellungen...
lock-screen = Sperrbildschirm
lock-screen-shortcut = Super + Esc
log-out = Abmelden
log-out-shortcut = Strg + Alt + Entf
suspend = Bereitschaft
restart = Neustart
shutdown = Ausschalten
confirm = Bestätigen
cancel = Abbrechen
confirm-question = Sind Sie sicher? { $action ->
        [restart] Ihr System wird
        [suspend] Ihr System wird
        [shutdown] Ihr System wird
        [lock-screen] Ihr Bildschirm wird 
        [log-out] Sie werden
        *[other] Die gewählte Aktion wird
    } in { $countdown } Sekunden automatisch { $action ->
        [restart] neugestartet
        [suspend] in Bereitschaft versetzt
        [shutdown] ausgeschaltet
        [lock-screen] gesperrt
        [log-out] abgemeldet
        *[other] ausgeführt
    }.
