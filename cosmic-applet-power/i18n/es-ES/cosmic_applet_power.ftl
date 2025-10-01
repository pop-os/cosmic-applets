power = Energía
settings = Configuración...
lock-screen = Bloquear pantalla
lock-screen-shortcut = Super + Escape
log-out = Cerrar sesión
log-out-shortcut = Super + Shift + Escape
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
    ¿{ $action ->
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] { shutdown }
        [log-out] ¿Cerrar todas las aplicaciones y cerrar la sesión
       *[other] ¿Realizar la acción seleccionada
    }?
confirm-body =
    { $action ->
        [restart] El ordenador se reiniciará
        [suspend] El ordenador se suspenderá
        [shutdown] El ordenador se apagará
        [lock-screen] La pantalla se bloqueará
        [log-out] La sesión se cerrará
       *[other] La acción seleccionada se realizará
    } automáticamente en { $countdown } segundos.
