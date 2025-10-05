power = الطاقة
settings = الإعدادات...
lock-screen = قفل الشاشة
lock-screen-shortcut = Super + Escape
log-out = تسجيل الخروج
log-out-shortcut = Super + Shift + Escape
suspend = تعليق
restart = إعادة التشغيل
shutdown = إيقاف التشغيل
confirm = تأكيد
cancel = إلغاء
confirm-button =
    { $action ->
        [restart] إعادة التشغيل
        [suspend] تعليق
        [shutdown] إيقاف التشغيل
        [log-out] تسجيل الخروج
       *[other] تأكيد
    }
confirm-title =
    { $action ->
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] { shutdown }
        [log-out] إغلاق جميع التطبيقات وتسجيل الخروج
       *[other] تنفيذ الإجراء المحدد
    } الآن؟
confirm-body =
    سيقوم النظام بـ { $action ->
        [restart] إعادة التشغيل
        [suspend] تعليق
        [shutdown] إيقاف التشغيل
        [lock-screen] قفل الشاشة
        [log-out] تسجيل الخروج
       *[other] تطبيق الإجراء المحدد
    } آليًا خلال { $countdown } ثانية.
