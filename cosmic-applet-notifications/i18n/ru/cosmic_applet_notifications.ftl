hours-ago =
    { $duration ->
        [0] Только что
        [1] 1 час назад
       *[other] { $duration } ч. назад
    }
minutes-ago =
    { NUMBER($duration) ->
        [1] 1 минуту назад
       *[other] { $duration } мин. назад
    }
show-less = Свернуть
show-more = Показать ещё { $more }
clear-all = Очистить все уведомления
do-not-disturb = Не беспокоить
notification-settings = Параметры уведомлений...
no-notifications = Уведомлений нет
clear-group = Очистить группу
