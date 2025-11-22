hours-ago =
    { $duration ->
        [0] Alldeles nyss
        [one] timma sedan
       *[other] { $duration } timmar sedan
    }
minutes-ago =
    { $duration ->
        [0] Alldeles nyss
        [one] 1 minut sedan
       *[other] { $duration } minuter sedan
    }
show-less = Visa mindre
show-more = Visa { $more } till
clear-group = Töm grupp
clear-all = Töm alla aviseringar
do-not-disturb = Stör ej
notification-settings = Aviseringsinställningar...
no-notifications = Inga aviseringar
