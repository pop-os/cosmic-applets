power = Energía
settings = Configuración...
lock-screen = Bloquear pantalla
lock-screen-shortcut = Súper + Escape
log-out = Cerrar sesión
suspend = Suspender
restart = Reiniciar
shutdown = Apagar
confirm = Confirmar
cancel = Cancelar
confirm-button = {
    $action -> 
        [restart] { restart }
        [suspend] { suspend}
        [shutdown] Apagar
        [log-out] { log-out }
        *[other] { confirm}
}
confirm-title = 
    ¿{ $action -> 
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] { shutdown }
        [log-out] Cerrar todas las aplicaciones y cerrar sesión
        *[other] Realizar la sesión seleccionada
    } ahora?
confirm-body = 
    El sistema { $action ->
        [restart] se reiniciará
        [suspend] se suspenderá
        [shutdown] se apagará
        [lock-screen] bloqueará la pantalla
        [log-out] cerrará sesión
        *[other] realizará la acción seleccionada
    } automáticamente en { $countdown } segundos.
