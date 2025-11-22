hours-ago =
    { $duration ->
        [0] Právě teď
        [one] Před 1 hodinou
       *[other] Před { $duration } hodinami
    }
minutes-ago =
    { $duration ->
        [0] Právě teď
        [one] Před 1 minutou
       *[other] Před { $duration } minutami
    }
show-less = Zobrazit méně
show-more =
    Zobrazit { $more } { $more ->
        [one] další
        [few] další
       *[other] dalších
    }
clear-group = Vymazat skupinu
clear-all = Vymazat všechna oznámení
do-not-disturb = Nerušit
notification-settings = Nastavení oznámení...
no-notifications = Žádná oznámení
