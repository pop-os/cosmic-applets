hours-ago =
    { $duration ->
        [0] Agora
        [one] 1 hora atrás
       *[other] { $duration } horas atrás
    }
minutes-ago =
    { NUMBER($duration) ->
        [1] 1 Minuto atrás
       *[other] { $duration } Minutos atrás
    }
show-less = Mostrar menos
show-more = Mostrar { $more } mais
clear-group = Limpar grupo de notificações
clear-all = Limpar todas as notificações
do-not-disturb = Não incomodar
notification-settings = Definições de notificações...
no-notifications = Sem notificações
