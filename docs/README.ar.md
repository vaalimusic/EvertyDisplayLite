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

لا يفرض إصدار Lite الرسمي **أي حد مصطنع متعلق بالمنتج أو الترخيص** على عدد
الشاشات الافتراضية، كما يدعم تخطيطات الشاشات المتعددة. يدعم برنامج التشغيل
MttVDD الموقّع حاليًا ما يصل إلى **خمس شاشات افتراضية نشطة** كحد تقني، ويتيح
EvertyDisplay كامل هذه الإمكانية ويمنع المحاولة السادسة بوضوح داخل الواجهة.

## البناء من المصدر

يلزم Windows 10/11 x64 وRust المستقر بأدوات MSVC وVisual Studio C++ Build Tools
وWindows SDK حديث.

```powershell
cargo build --workspace --release
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

تنزّل آلية بناء المثبّت MttVDD 24.12.24 من الإصدار الرسمي للمشروع الأصلي، ثم
تتحقق من قيمة SHA-256 المثبتة وتوقيعات Authenticode قبل تضمينه. لا تُحفظ مفاتيح
التوقيع في المستودع مطلقًا. يمكن بناء المثبّت محليًا بواسطة
`./scripts/build-installer.ps1` مع Inno Setup 6.

## ترخيص copyleft قوي

يُنشر المشروع حصريًا وفق **GNU GPLv3**. عند توزيع نسخة معدلة أو مشتقة، يجب توفير
المصدر المقابل للمستلمين مع الحريات نفسها. النص الكامل في [LICENSE](../LICENSE)
هو المرجع القانوني؛ هذا الملخص ليس استشارة قانونية.

المؤلف: **Arthur Valiev — Артур Валиев** · [info@everty.ru](mailto:info@everty.ru)

</div>
