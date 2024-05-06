power = Energia
settings = Definições...
lock-screen = Bloquear ecrã
lock-screen-shortcut = Super + Escape
log-out = Terminar sessão
log-out-shortcut = Ctrl + Alt + Delete
suspend = Suspender
restart = Reiniciar
shutdown = Encerrar
confirm = Confirmar
cancel = Cancelar
confirm-body = 
    { $action ->
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] { shutdown }
        [lock-screen] A bloquear o ecrã
        [log-out] A terminar sessão
        *[other] A ação selecionada
    } continuará em { $countdown } segundos.

