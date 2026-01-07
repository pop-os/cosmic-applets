hours-ago =
    { $duration ->
        [0] Agora mesmo
        [one] 1 hora atrás
       *[other] { $duration } horas atrás
    }
minutes-ago =
    { NUMBER($duration) ->
        [1] 1 minuto atrás
       *[other] { $duration } minutos atrás
    }
show-less = Mostrar menos
show-more = Mostrar { $more } mais
clear-group = Limpar grupo
clear-all = Limpar todas as notificações
do-not-disturb = Não Perturbe
notification-settings = Configurações de notificações...
no-notifications = Sem notificações
