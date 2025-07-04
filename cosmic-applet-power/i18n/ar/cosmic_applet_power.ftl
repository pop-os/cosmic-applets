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
confirm-button = {
  \$action ->
   \[restart] إعادة التشغيل
   \[suspend] تعليق
   \[shutdown] إيقاف التشغيل
   \[log-out] تسجيل الخروج
   \*\[other] تأكيد
}
confirm-title =
  { \$action ->
   \[restart] إعادة التشغيل
   \[suspend] تعليق
   \[shutdown] إيقاف التشغيل
   \[log-out] إغلاق جميع التطبيقات وتسجيل الخروج
   \*\[other] تنفيذ الإجراء المحدد
  } الآن؟
confirm-body =
  سيقوم النظام بـ{ \$action ->
   \[restart] إعادة التشغيل
   \[suspend] التعليق
   \[shutdown] الإيقاف
   \[lock-screen] قفل الشاشة
   \[log-out] تسجيل الخروج
   \*\[other] تنفيذ الإجراء المحدد
  } تلقائيًا خلال { \$countdown } ثانية.
