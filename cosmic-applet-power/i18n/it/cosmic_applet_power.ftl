power = Energia
settings = Impostazioni...
lock-screen = Schermata di blocco
lock-screen-shortcut = Super + ESC
log-out = Disconnetti
log-out-shortcut = Ctrl + Alt + CANC
suspend = Sospendi
restart = Riavvia
shutdown = Spegni
confirm = Conferma
cancel = Annulla
confirm-question = 
    Confermi? { $action ->
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] { shutdown }
        [lock-screen] Blocco schermo in corso
        [log-out] Disconnessione in corso
        *[other] L'azione selezionata
    } verrà eseguita tra { $countdown } secondi.

