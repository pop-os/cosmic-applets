hours-ago =
    { NUMBER($duration) ->
        [one] { $duration } годину тому
        [few] { $duration } години тому
       *[other] { $duration } годин тому
    }
minutes-ago =
    { NUMBER($duration) ->
        [one] { $duration } хвилину тому
        [few] { $duration } хвилини тому
       *[other] { $duration } хвилин тому
    }
show-less = Показати менше
show-more = Показати ще { $more }
clear-group = Очистити групу
clear-all = Очистити всі сповіщення
do-not-disturb = Не турбувати
notification-settings = Параметри сповіщень...
no-notifications = Немає сповіщень
