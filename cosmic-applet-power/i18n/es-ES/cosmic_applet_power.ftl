power = Alimentación
settings = Ajustes...
lock-screen = Bloquear pantalla
lock-screen-shortcut = Super + Escape
log-out = Cerrar sesión
suspend = Suspender
restart = Reiniciar
shutdown = Apagar
confirm = Confirmar
cancel = Cancelar
confirm-button = {
    $action ->
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] { shutdown }
        [log-out] { log-out }
        *[other] { confirm }
}
confirm-title =
    ¿{ $action ->
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] { shutdown }
        [log-out] Salir de todas las aplicaciones y la sesión
        *[other] Realizar la acción seleccionada
    } ahora?
confirm-body = 
    { $action ->
        [restart] El ordenador se reiniciará
        [suspend] El ordenador se suspenderá
        [shutdown] El ordenador se apagará
        [lock-screen] La pantalla se bloqueará
        [log-out] La sesión se cerrará
        *[other] La acción seleccionada se realizará
    } automáticamente en { $countdown } segundos.
