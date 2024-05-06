power = Virta
settings = Asetukset...
lock-screen = Lukitusnäyttö
lock-screen-shortcut = Super + Escape
log-out = Kirjaudu ulos
log-out-shortcut = Ctrl + Alt + Delete
suspend = Lepotila
restart = Uudelleenkäynnistä
shutdown = Sammuta
confirm = Varmista
cancel = Peruuta
confirm-body = 
    Oletko Varma? { $action ->
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] { shutdown }
        [lock-screen] Lukitaan näyttö
        [log-out] Kirjaudutaan Ulos
        *[other] Valittu Toiminta
    } jatketaan { $countdown } sekunnin kuluttua.

