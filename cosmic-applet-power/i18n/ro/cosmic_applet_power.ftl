power = Putere
settings = Setări...
lock-screen = Blocare ecran
lock-screen-shortcut = Super + Escape
log-out = Deconectare
log-out-shortcut = Super + Shift + Escape
suspend = Suspendare
restart = Repornire
shutdown = Oprire
confirm = Confirmă
cancel = Anulează
confirm-button = {
    $action -> 
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] Oprire
        [log-out] { log-out }
        *[other] { confirm }
}
confirm-title = 
    { $action -> 
        [restart] Repornește
        [suspend] Suspendă
        [shutdown] Oprește
        [log-out] Închide toate aplicațiile și deconectează-te
        *[other] Aplică acțiunea selectată
    } acum?
confirm-body = 
    Sistemul va { $action ->
        [restart] fi repornit
        [suspend] fi suspendat
        [shutdown] fi oprit
        [lock-screen] avea ecranul blocat
        [log-out] fi deconectat 
        *[other] aplica acțiunea selectată
    } automat în { $countdown } secunde.
