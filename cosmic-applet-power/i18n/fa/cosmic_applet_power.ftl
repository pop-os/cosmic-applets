power = نیرو
settings = تنظیمات...
lock-screen = قفل صفحه
lock-screen-shortcut = Super + Escape
log-out = خروج از حساب
log-out-shortcut = Super + Shift + Escape
suspend = تعلیق کردن
restart = راه‌اندازی مجدد
shutdown = خاموش کردن
confirm = تأیید
cancel = لغو
confirm-button = {
    $action -> 
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] { shutdown }
        [log-out] { log-out }
        *[other] { confirm }
}
confirm-title = 
    هم‌اکنون { $action -> 
        [restart] { restart }
        [suspend] تعلیق
        [shutdown] خاموش
        [log-out] برنامه‌ها بسته و از حساب خارج
        *[other] عمل انتخاب شده اعمال
    } شود؟
confirm-body = 
    سیستم تا { $countdown } ثانیه دیگر به طور خودکار { $action ->
        [restart] راه‌اندازی مجدد خواهد شد.
        [suspend] تعلیق خواهد شد.
        [shutdown] خاموش خواهد شد.
        [lock-screen] قفل خواهد شد.
        [log-out] از حساب خارج خواهد شد.
        *[other] عمل انتخاب شده را اعمال خواهد کرد.
    }

