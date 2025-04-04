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
confirm-button = {
    $action -> 
        [restart] { restart }
        [suspend] { suspend}
        [shutdown] Spegni
        [log-out] { log-out }
        *[other] { confirm}
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
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] { shutdown }
        [lock-screen] Blocco schermo in corso
        [log-out] Disconnessione in corso
        *[other] L'azione selezionata
    } verrà eseguita tra { $countdown } secondi.

