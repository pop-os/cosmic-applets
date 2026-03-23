confirm = ਤਸਦੀਕ
cancel = ਰੱਦ ਕਰੋ
suspend = ਸਸਪੈਂਡ
restart = ਮੁੜ-ਚਾਲੂ
log-out = ਲਾਗ ਆਉਟ
shutdown = ਬੰਦ ਕਰੋ
power = ਪਾਵਰ
settings = ਸੈਟਿੰਗਾਂ...
lock-screen = ਸਕਰੀਨ ਲਾਕ
lock-screen-shortcut = ਸੁਪਰ + Esc
confirm-button =
    { $action ->
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] Power off
        [log-out] { log-out }
       *[other] { confirm }
    }
confirm-title =
    { $action ->
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] { shutdown }
        [log-out] Quit all applications and log out
       *[other] Apply the selected action
    } now?
confirm-body =
    ਸਿਸਟਮ  { $countdown } ਸਕਿੰਟਾਂ ਵਿਚ ਆਪਣੇ-ਆਪ { $action ->
        [restart] ਮੁੜ-ਚਾਲੂ ਹੋਵੇਗਾ
        [suspend] ਸਸਪੈਂਡ ਹੋਵੇਗਾ
        [shutdown] ਬੰਦ ਹੋਵੇਗਾ
        [lock-screen] ਸਕਰੀਨ ਲਾਕ ਕਰੇਗਾ
        [log-out] ਲਾਗ ਆਉਟ ਕਰੇਗਾ
       *[other] ਚੁਣੀ ਕਾਰਵਾਈ ਲਾਗੂ ਕਰੇਗਾ
    }।
log-out-shortcut = Super + Shift + Escape
