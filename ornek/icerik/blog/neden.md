---
baslik: Neden bağımlılıksız bir üretici?
tarih: 2026-09-29
ozet: Bağımlılık ağacı yıllar sonra aynı sonucu vermez; küçük ve sabit bir araç verir.
yazar: web ekibi
etiketler: [surec, tasinabilirlik]
siralama: 2
---

Node tabanlı üreticilerin ilk kurulumu paket sayısına bağlı olarak dakikalarca
sürer ve internet gerektirir. Kapanmış ya da kısıtlı ağda çalışan bir kurum bu
adımı tekrarlayamaz.

## Aynı girdiden aynı çıktı

Bir site şu kuralı sağlıyorsa dağıtım bir dosya kopyalama işlemine indirgenir:

1. İçerik klasörü = tek doğruluk kaynağı.
2. Üretim çıktısı = dosya sunucusuna doğrudan yüklenebilen bir klasör.
3. Sunucu türü, veritabanı, PHP ya da çalışma zamanı desteği yok.

## Bellek notu

Tüm sayfaların bellekte tutulmaması bilinçli bir seçimdir. Yalnızca sayfa
**meta verisi** saklanır; gövdeler akış hâlinde okunur ve sayfaya yazıldıktan
sonra serbest bırakılır.

```text
icerik/  ->  on bilgi  ->  markdown  ->  sablon  ->  dist/
             (meta)      (akis)       (saf)      (atomik)
```

## Kısmi yeniden üretim

Her içerik dosyasının `mtime` ve boyut imzası saklanır. Değişmeyen dosya
yeniden ayrıştırılmaz:

```text
siteturk build   ->  yeniden uretilen: 0 · korunan: 4
```

Önbelleğe güvenilmiyorsa `--tam-uretim` kipi her şeyi yeniden yazar.
