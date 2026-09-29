---
baslik: StatikUsta ile tanışın
tarih: 2026-09-29
ozet: Klasör dolusu Markdown dosyasını, kurulum gerektirmeyen bir statik siteye dönüştüren üretici.
yazar: web ekibi
etiketler:
  - tanitim
  - statik-site
siralama: 1
---

## Ne yapar?

StatikUsta, bir klasördeki `.md` dosyalarını okur, ön bilgi başlıklarını
ayrıştırır, gövdeleri HTML'e çevirir ve sonucu tek bir klasöre yazar. Kullanılan
şeyler: Rust standart kütüphanesi, `serde` ve `clap`. Başka hiçbir şey yok.

**Hiçbir kurulum adımı yoktur.** Node, npm, Ruby, Python, Go veya .NET
çalışma zamanına ihtiyaç duyulmaz. Kurum içi, çevrimdışı ortamlarda da aynı
sonucu verir.

## Nasıl çalışır?

| Adım | Ne olur |
| --- | --- |
| 1 | `siteturk.json` okunur; yoksa varsayılanlar kullanılır |
| 2 | `icerik/` altındaki `.md` dosyaları **yol sırasına** göre keşfedilir |
| 3 | Ön bilgi başlıkları ve Markdown gövdesi ayrıştırılır |
| 4 | Şablon çalıştırılır; sayfa `dist/` altına **atomik** olarak yazılır |
| 5 | `sitemap.xml` ve `robots.txt` üretilir |

Adlandırma kuralı tek ve öngörülebilirdir:

```text
icerik/hakkimizda.md      ->  dist/hakkimizda/index.html  ->  /hakkimizda/
icerik/blog/neden.md      ->  dist/blog/neden/index.html  ->  /blog/neden/
icerik/index.md           ->  dist/index.html             ->  /
```

## Listeler

Sıralı ve sırasız listeler aynı biçimde işlenir:

1. Ön bilgi okunur.
2. Gövde HTML'e çevrilir.
3. Şablon çalıştırılır.

* Boş satırlar ayraçtır.
* Girintili öğeler iç içe listeye dönüşür.
* Uzun satırlar satır sonunda bozulmaz.

## Kod blokları

```rust
let cikti = uret(&kok, &yapilandirma, UretimSecenekleri::default())?;
println!("{} sayfa", cikti.rapor.toplam_dosya);
```

Satır içi kod da çalışır: `icerik/`, `dist/`, `siteturk.json`.

> Alıntılar `>` ile yazılır ve `<blockquote>` olarak üretilir.

---

Yatay çizgi `---` ile yazılır. Ön bilgi bloğu da `---` ile sınırlanır; ikisi
karışmaz, çünkü ön bilgi yalnızca dosyanın **en üstünde** aranır.
