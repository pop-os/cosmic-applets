power = Energia
settings = Configurações...
lock-screen = Bloquear Tela
lock-screen-shortcut = Super + Escape
log-out = Sair
suspend = Suspender
restart = Reiniciar
shutdown = Desligar
confirm = Confirmar
cancel = Cancelar
confirm-button = {
    $action -> 
        [restart] { restart }
        [suspend] { suspend}
        [shutdown] Desligar
        [log-out] { log-out }
        *[other] { confirm}
}
confirm-title = 
    { $action -> 
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] { shutdown }
        [log-out] Fechar todos os aplicativos e Sair
        *[other] Aplicar a ação selecionada
    } agora?
confirm-body = 
    O sistema irá { $action ->
        [restart] reiniciar
        [suspend] suspender
        [shutdown] desligar
        [lock-screen] bloquear a tela
        [log-out] sair
        *[other] aplicar a ação selecionada
    } automaticamente em { $countdown } segundos.

