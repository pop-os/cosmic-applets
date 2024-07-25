power = Живлення
settings = Налаштування...
lock-screen = Заблокувати екран
lock-screen-shortcut = Super + Escape
log-out = Вийти
log-out-shortcut = Ctrl + Alt + Delete
suspend = Призупинити
restart = Перезавантажити
shutdown = Вимкнути
confirm = Підтвердити
cancel = Скасувати
confirm-button = {
    $action -> 
        [restart] { restart }
        [suspend] { suspend}
        [shutdown] Вимкнути
        [log-out] { log-out }
        *[other] { confirm}
}
confirm-title = 
    { $action -> 
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] { shutdown }
        [log-out] Закрити всі застосунки та вийти
        *[other] Виконати вибрану дію
    } зараз?
confirm-body = 
    Система { $action ->
        [restart] перезавантажиться
        [suspend] призупиниться
        [shutdown] вимкнеться
        [lock-screen] заблокує екран
        [log-out] виконає вихід
        *[other] Виконати вибрану дію
    } автоматично за { $countdown } секунд.

