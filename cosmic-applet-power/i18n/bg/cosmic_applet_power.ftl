power = Захранване
settings = Настройки...
lock-screen = Заключване на екрана
lock-screen-shortcut = Super + Escape
log-out = Изход
log-out-shortcut = Super + Shift + Escape
suspend = Приспиване
restart = Рестартиране
shutdown = Изключване
confirm = Потвърждаване
cancel = Отказване
confirm-button = {
    $action -> 
        [restart] { restart }
        [suspend] { suspend}
        [shutdown] Изключване
        [log-out] { log-out }
        *[other] { confirm}
}
confirm-title = 
    { $action -> 
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] { shutdown }
        [log-out] Спиране на всички програми и изход
        *[other] Прилагане на избраното действие
    } сега?
confirm-body = 
    Системата ще { $action ->
        [restart] се рестартира
        [suspend] се приспи
        [shutdown] се изключи
        [lock-screen] се заключи
        [log-out] излезе от сесията
        *[other] приложи избраното действие
    } автоматично след { $countdown } секунди.