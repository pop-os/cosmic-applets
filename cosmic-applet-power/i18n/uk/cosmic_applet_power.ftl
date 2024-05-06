power = Живлення
settings = Параметри...
lock-screen = Заблокувати екран
lock-screen-shortcut = Super + Escape
log-out = Вийти
log-out-shortcut = Ctrl + Alt + Delete
suspend = Призупинити
restart = Перезавантажити
shutdown = Вимкнути
confirm = Підтвердити
cancel = Скасувати
confirm-body = 
    Are you sure? { $action ->
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] { shutdown }
        [lock-screen] Блокування екрана
        [log-out] Вихід
        *[other] Вибрана дія
    } продовжиться через { $countdown } секунд.

