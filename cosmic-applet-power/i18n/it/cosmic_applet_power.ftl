power = Energia
settings = Impostazioni...
lock-screen = Schermata di blocco
lock-screen-shortcut = Super + ESC
log-out = Disconnetti
log-out-shortcut = Super + Shift + ESC
suspend = Sospendi
restart = Riavvia
shutdown = Spegni
confirm = Conferma
cancel = Annulla
confirm-button =
    { $action ->
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] Spegni
        [log-out] { log-out }
       *[other] { confirm }
    }
confirm-title =
    { $action ->
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] { shutdown }
        [log-out] Chiudi tutte le applicazioni e termina la sessione
       *[other] Applica l'azione selezionata
    } now?
confirm-body =
    { $action ->
        [restart] Riavvia
        [suspend] Sospendi
        [shutdown] Spegni
        [lock-screen] Blocco schermo
        [log-out] Termina sessione
       *[other] L'azione selezionata verrà applicata
    } automaticamente tra { $countdown } secondi.
