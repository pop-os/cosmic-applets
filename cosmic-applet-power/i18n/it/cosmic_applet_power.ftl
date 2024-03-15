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
        [restart] Il sistema si riavvierà
        [suspend] Il sistema si sospenderà
        [shutdown] Il sistema si spegnerà
        [lock-screen] Lo schermo si blochherà
        [log-out] Si disconetterà
        *[other] L'azione selezionata verrà eseguita
    } tra { $countdown ->
    	[1] 1 secondo.
    	*[other] {$countdown} secondi.
    }


