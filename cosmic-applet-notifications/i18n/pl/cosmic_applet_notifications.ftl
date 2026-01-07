hours-ago =
    { $duration ->
        [0] Przed chwilą
        [one] 1 godzinę temu
        [few] { $duration }  godziny temu
       *[other] { $duration } godzin temu
    }
minutes-ago =
    { $duration ->
        [0] Przed chwilą
        [one] 1 minutę temu
        [few] { $duration } minuty temu
       *[other] { $duration } minut temu
    }
show-less = Pokaż mniej
show-more = Pokaż { $more } więcej
clear-group = Wyczyść grupę
clear-all = Wyczyść wszystkie powiadomienia
do-not-disturb = Nie przeszkadzać
notification-settings = Ustawienia powiadomień…
no-notifications = Brak powiadomień
