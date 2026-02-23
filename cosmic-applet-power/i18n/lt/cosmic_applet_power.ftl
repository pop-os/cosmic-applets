confirm-button =
    { $action ->
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] Išjungti
        [log-out] { log-out }
       *[other] { confirm }
    }
power = Maitinimas
confirm-body =
    Sistema { $action ->
        [restart] įsijungs iš naujo
        [suspend] užmigs
        [shutdown] išsijungs
        [lock-screen] užrakins ekraną
        [log-out] atjungs dabartinį naudotoją
       *[other] taikys pasirinktą veiksmą
    } automatiškai po { $countdown } sekundžių.
lock-screen = Užrakinti Ekraną
log-out = Atsijungti
restart = Paleisti iš naujo
log-out-shortcut = Super + Shift + Escape
cancel = Atšaukti
confirm = Patvirtinti
settings = Nustatymai...
lock-screen-shortcut = Super + Escape
confirm-title =
    { $action ->
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] { shutdown }
        [log-out] Uždaryti visas aplikacijas ir atsijungti
       *[other] Taikyti pasirinktą veiksmą
    } now?
shutdown = Išjungti
suspend = Miego režimas
