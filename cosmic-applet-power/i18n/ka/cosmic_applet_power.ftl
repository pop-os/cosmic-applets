restart = გადატვირთვა
suspend = ძილი
cancel = გაუქმება
shutdown = გამორთვა
confirm = დადასტურება
power = კვება
settings = მორგება...
log-out = ანგარიშიდან გასვლა
lock-screen = ეკრანის ჩაკეტვა
lock-screen-shortcut = Super + Escape
log-out-shortcut = Super + Shift + Escape
confirm-button =
    { $action ->
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] გამორთვა
        [log-out] { log-out }
       *[other] { confirm }
    }
confirm-title =
    { $action ->
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] { shutdown }
        [log-out] ყველა აპლიკაციის გამორთვა და ანგარიშიდან გასვლა
       *[other] მონიშნული ქმედების გადატარება
    } now?
confirm-body =
    სისტემა { $action ->
        [restart] გადაიტვირთება
        [suspend] დაიძინებს
        [shutdown] გამოირთვება
        [lock-screen] დაბლოკავს ეკრანს
        [log-out] გავა თქვენი ანგარიშიდან
       *[other] გადაატარებს არჩეულ ქმედებას
    } ავტომატურად { $countdown } წამში.
