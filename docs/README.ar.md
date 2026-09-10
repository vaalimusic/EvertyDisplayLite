<div dir="rtl" align="center">
  <img src="../assets/logo_mark.svg" width="96" alt="شعار EvertyDisplay Lite">
  <h1>EvertyDisplay Lite</h1>
  <p><strong>شاشة إضافية، من دون أجهزة إضافية.</strong></p>
  <p><a href="../README.md">English</a> · <a href="README.ru.md">Русский</a> · العربية · <a href="README.es.md">Español</a> · <a href="README.de.md">Deutsch</a> · <a href="README.fr.md">Français</a></p>
</div>

<div dir="rtl">

ينشئ EvertyDisplay Lite شاشة افتراضية إضافية في Windows تتصرف كالشاشة
الفعلية: يمكنك نقل المؤشر والنوافذ عبر حدودها وترتيبها مكانيًا مع بقية الشاشات.

## المزايا

- تخطيط مكاني أصلي في Windows؛
- نقل النوافذ عبر Drag-to-Teleport؛
- نافذة Live PiP تحفظ الحجم والموقع؛
- Viewport بملء الشاشة باستخدام Direct3D 11؛
- إشعارات OSD واختصارات ووضع للألعاب؛
- استعادة آمنة بعد تعطل برنامج التشغيل أو انقطاع العملية؛
- واجهة بست لغات.

يدعم الإصدار الرسمي Lite **شاشة افتراضية نشطة واحدة**. يفرض هذا الحد داخل
الخدمة الخلفية، وليس في الواجهة فقط، لذلك لا يمكن لطلب IPC مباشر أو إعداد قديم
إعادة شاشة إضافية. تُعطّل الهويات الزائدة من دون حذف النسخة الاحتياطية.

## البناء من المصدر

يلزم Windows 10/11 x64 وRust المستقر بأدوات MSVC وVisual Studio C++ Build Tools
وWindows SDK حديث.

```powershell
cargo build --workspace --release
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

لا يحتوي المستودع عمدًا على مفاتيح التوقيع أو الشهادات أو `devcon.exe` أو حزمة
برنامج التشغيل الموقعة. يتوفر برنامج التشغيل المدعوم في الإصدار الرسمي على
[صفحة EvertyDisplay](https://desk.everty.ru/evertydisplay).

## ترخيص copyleft قوي

يُنشر المشروع حصريًا وفق **GNU GPLv3**. عند توزيع نسخة معدلة أو مشتقة، يجب توفير
المصدر المقابل للمستلمين مع الحريات نفسها. النص الكامل في [LICENSE](../LICENSE)
هو المرجع القانوني؛ هذا الملخص ليس استشارة قانونية.

المؤلف: **Arthur Valiev — Артур Валиев** · [info@everty.ru](mailto:info@everty.ru)

</div>
