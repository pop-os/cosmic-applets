hours-ago = { NUMBER($duration) -> 
    [1] pred hodinou
    *[other] pred {$duration} hodinami
}
minutes-ago = { NUMBER($duration) -> 
    [1] pred minútou
    *[other] pred {$duration} minútami
}
show-less = Zobraziť menej
show-more = Zobraziť {$more} ďalšie 
clear-group = Vymazať skupinu
clear-all = Vymazať všetky notifikácie
do-not-disturb = Nerušiť
notification-settings = Nastavenia notifikácií...
no-notifications = Žiadne notifikácie