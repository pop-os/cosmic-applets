power = Enerxía
settings = Axustes...
lock-screen = Pantalla de Bloqueo
lock-screen-shortcut = Super + Esc
log-out = Pechar Sesión
log-out-shortcut = Super + Maiús + Esc
suspend = Suspender
restart = Reiniciar
shutdown = Apagar
confirm = Confirmar
cancel = Cancelar
confirm-button =
    { $action ->
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] Apagar
        [log-out] { log-out }
       *[other] { confirm }
    }
confirm-title =
    { $action ->
        [restart] { restart } agora?
        [suspend] { suspend } agora?
        [shutdown] { shutdown } agora?
        [log-out] Pechar todas as aplicacións e pechar a sesión agora?
       *[other] Aplicar a acción seleccionada agora
    }
confirm-body =
    O sistema { $action ->
        [restart] reiniciarase
        [suspend] suspenderase
        [shutdown] apagarase
        [lock-screen] bloqueará a pantalla
        [log-out] pechará a sesión
       *[other] aplicará a acción seleccionada
    } automaticamente en { $countdown } segundos.
