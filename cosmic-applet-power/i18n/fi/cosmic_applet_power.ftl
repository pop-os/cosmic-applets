power = Virta
settings = Asetukset...
lock-screen = Lukitusnäyttö
lock-screen-shortcut = Super + Escape
log-out = Kirjaudu ulos
suspend = Lepotila
restart = Käynnistä uudelleen
shutdown = Sammuta
confirm = Vahvista
cancel = Peru
confirm-body =
    { $action ->
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] { shutdown }
        [lock-screen] Lukitaan näyttö
        [log-out] Kirjaudutaan Ulos
       *[other] Valittu Toiminta
    } jatketaan { $countdown } sekunnin kuluttua.
