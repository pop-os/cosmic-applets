power = Virta
settings = Asetukset…
lock-screen = Lukitse näyttö
lock-screen-shortcut = Super + Esc
log-out = Kirjaudu ulos
suspend = Lepotila
restart = Käynnistä uudelleen
shutdown = Sammuta
confirm = Vahvista
cancel = Peru
confirm-body =
    Järjestelmä { $action ->
        [restart] käynnistyy uudelleen
        [suspend] siirtyy lepotilaan
        [shutdown] sammuttaa virran
        [lock-screen] lukitsee näytön
        [log-out] kirjautuu ulos
       *[other] aikoo tehdä valitun toimen
    } { $countdown } sekunnin kuluttua.
confirm-button =
    { $action ->
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] Sammuta
        [log-out] { log-out }
       *[other] { confirm }
    }
log-out-shortcut = Super + Vaihto + Esc
confirm-title =
    { $action ->
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] { shutdown }
        [log-out] Lopeta kaikki sovellukset ja kirjaudu ulos
       *[other] Toteuta valittu toiminto
    } nyt?
