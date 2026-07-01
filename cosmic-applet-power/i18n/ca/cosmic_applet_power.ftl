cancel = Cancel·lar
confirm = Confirmar
restart = Reinicia
suspend = Suspèn
confirm-button =
    { $action ->
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] Apaga
        [log-out] { log-out }
       *[other] { confirm }
    }
power = Energia
confirm-body =
    { $action ->
        [restart] L'ordinador es reiniciarà
        [suspend] L'ordinador se suspendrà
        [shutdown] L'ordinador s'apagarà
        [lock-screen] La pantalla es bloquejarà
        [log-out] La sessió es tancarà
       *[other] L'acció seleccionada es durà a terme
    } automàticament d'aquí a { $countdown } segons.
lock-screen = Bloqueja la pantalla
shutdown = Apaga
log-out = Tanca la sessió
log-out-shortcut = Súper + Majús + Esc
settings = Configuració...
lock-screen-shortcut = Súper + Esc
confirm-title =
    ¿{ $action ->
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] { shutdown }
        [log-out] Voleu tancar totes les aplicacions i tancar la sessió
       *[other] Voleu dur a terme l'acció seleccionada
    }?
