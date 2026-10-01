# StatikUsta (Siteturk)

Klasör dolusu Markdown dosyasını, **kurulum gerektirmeyen** bir statik siteye
dönüştüren tek dosyalık üretici. Rust standart kütüphanesi, `serde` ve `clap`
dışında hiçbir şey kullanılmaz: Node, npm, Ruby, Python, Go veya .NET çalışma
zamanına ihtiyaç duyulmaz, ağ erişimi gerekmez, `pulldown-cmark` ya da hazır bir
şablon motoru kullanılmaz. Markdown alt kümesi ve şablon dili projeye özel olarak
yazılmıştır.

## Özellikler

- **Kendi CommonMark alt kümesi** — başlık (ATX ve setext), paragraf, sıralı ve
  sırasız liste (iç içe), alıntı, kod bloğu (kapatmalı ve girintili), yatay çizgi,
  tablo; satır içi `kod`, bağlantı, görsel, kalın, italik, otomatik bağlantı ve
  geri kaçış. Çıktı **ağaçsız, tek geçişli** üretilir.
- **Ön bilgi (front matter) ayrıştırma** — dosyanın en üstündeki `---` bloğu;
  `baslik`, `tarih`, `etiketler`, `ozet`, `yazar`, `taslak`, `slug`, `siralama`
  alanları. Tanımsız alan, iki noktasız satır veya kapanmayan blok **hata**
  üretir. Dosya başındaki UTF-8 BOM sessizce yutulur.
- **Sığ şablon dili** — `{{ degisken }}`, `{% if %}…{% else %}…{% endif %}`,
  `{% for x in liste %}…{% endfor %}`, `ham` / `varlik` / `kisalt` işlevleri ve
  `| ham` süzgeci. Varsayılan çıktı kaçışlıdır; tanımsız değişken hata verir.
- **Güvenlik sınırları** — döngü adımı (10.000), iç içe geçiş derinliği (32) ve
  çıktı boyutu (32 MiB) sınırlıdır; süreci tüketen şablon engellenir.
- **Varlık boru hattı** — `static/` klasörü çıktıya kopyalanır, isteğe bağlı
  içerik imzası (cache busting) ve boşluk kısaltma uygulanır, CSS/HTML içindeki
  göreli bağlantılar yeniden yazılır.
- **`sitemap.xml` ve `robots.txt` üretimi** — `adres` alanından mutlak adresler
  kurulur; `taslak: true` sayfalar `robots.txt` içinde `Disallow` edilir.
- **Kısmi yeniden üretim** — içerik dosyalarının `mtime` + boyut imzası
  `.siteturk-onbellek.json` içinde tutulur; değişmeyen sayfalar yeniden ayrıştırılmaz.
  `--tam-uretim` önbelleği yok sayar.
- **Dahili canlı yenileme sunucusu** — `std::net::TcpListener` ile, yalnızca
  `127.0.0.1` üzerinde. Sayfaya enjekte edilen küçük yoklama betiği, üretim
  sayacı değiştiğinde sayfayı yeniler. `tokio`/`hyper` kullanılmaz.
- **Dört komut** — `new`, `build`, `serve`, `check`.
- **Yalnız çıktı klasörüne yazar**; program dizini dışına hiçbir şey yazılmaz.
  Sayfa dosyaları geçici dosya üzerinden yazılıp **atomik** olarak taşınır.

## Kurulum

Gereken tek şey Rust araç zinciridir. Doğrulanmış MSRV sürümü **1.74**'tür
(bu makinede `cargo 1.98.1` ile derlenmiştir).

```console
$ cargo build --release
    Finished `release` profile [optimized] target(s) in 0.04s
```

İkili `target\release\siteturk.exe` olarak oluşur. Kalıcı olarak kurmak için:

```console
$ cargo install --path .
  Installing siteturk v0.1.0 (C:\...\projects\14-siteturk)
   Compiling siteturk v0.1.0
    Finished `release` profile [optimized] target(s) in 1.89s
  Installing %USERPROFILE%\.cargo\bin\siteturk.exe
   Installed package `siteturk v0.1.0 (...projects\14-siteturk)` (executable `siteturk.exe`)
```

`cargo install` komutu da çalışır; çıktı aynı `siteturk` adlı ikiliyi üretir ve
ardından `siteturk` komutu yolunuzda bulunur. Aşağıdaki örneklerde **kurulu**
`exe` yolu (`target\release\siteturk.exe`) yerine `siteturk` yazılmıştır.

## Kullanım

Depodaki `ornek/` klasörü, üreticinin üzerinde gerçekten çalıştığı örnek sitedir
(4 Markdown dosyası, 1 şablon, 1 varlık). Aşağıdaki **her komut bu depoda
gerçekten çalıştırılmıştır** ve çıktılar kopyadır. Komutlar `ornek/` klasöründe
çalıştırılmış, bu yüzden `-p ornek` yerine yalnızca alt komut yazılmıştır.

### 1. Sıfırdan yeni site

```console
$ siteturk new %USERPROFILE%\AppData\Local\Temp\st-yeni
olusturuldu: %USERPROFILE%\AppData\Local\Temp\st-yeni
simdi calistir: cd %USERPROFILE%\AppData\Local\Temp\st-yeni && siteturk build
```

### 2. Siteyi üret

```console
$ siteturk build -v
uretildi: 4 sayfa (yeniden 3 · korunan 0 · taslak 1 · varlik 1)
sure: 2 ms
  sayfa: /
  sayfa: /blog/neden/
  sayfa: /hakkimizda/
  varlik: /static/sayfa.92d54d9ed42603c0.css (2181 bayt)
```

(`sure` değeri çalıştırmadan çalıştırmaya değişir; sayılar ve dosya listesi
deterministiktir.)

Üretilen `dist` ağacı:

```console
$ Get-ChildItem -Recurse -File dist | ForEach-Object { $_.FullName.Replace((Resolve-Path dist).Path + "\", "") }
index.html
robots.txt
sitemap.xml
blog\neden\index.html
hakkimizda\index.html
static\sayfa.92d54d9ed42603c0.css
```

`icerik\blog\taslak-notlar.md` dosyası `taslak: true` taşıdığı için
**yazılmamıştır**; onun yerine `robots.txt` içinde engellenmiştir.

### 3. Kısmi yeniden üretim

Aynı komutu ikinci kez çalıştırmak hiçbir şeyi yeniden ayrıştırmaz:

```console
$ siteturk build
uretildi: 4 sayfa (yeniden 0 · korunan 3 · taslak 1 · varlik 1)
```

Önbelleği yok sayıp her şeyi yeniden yazmak için `--tam-uretim`:

```console
$ siteturk build --tam-uretim
uretildi: 4 sayfa (yeniden 3 · korunan 0 · taslak 1 · varlik 1)
```

### 4. Doğrulama (hiçbir şey yazmaz)

```console
$ siteturk check
yapilandirma: okundu
  kaynak: icerik
  cikti:  dist
  adres:  https://ornek.example
sablon:    soz dizimi gecerli
icerik:    4 sayfa, 0 uyari
kontrol:   basarili — hata yok
```

### 5. Üretilen site haritası ve robots.txt

```console
$ Get-Content dist\sitemap.xml -Raw
<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
  <url>
    <loc>https://ornek.example/</loc>
    <lastmod>2026-09-29</lastmod>
  </url>
  <url>
    <loc>https://ornek.example/blog/neden/</loc>
    <lastmod>2026-09-29</lastmod>
  </url>
  <url>
    <loc>https://ornek.example/hakkimizda/</loc>
    <lastmod>2026-09-29</lastmod>
  </url>
</urlset>
```

```console
$ Get-Content dist\robots.txt -Raw
User-agent: *
Allow: /
Disallow: /blog/taslak-notlar/

Sitemap: https://ornek.example/sitemap.xml
```

### 6. Yerel canlı yenileme sunucusu

```console
$ siteturk serve
siteturk serve: http://127.0.0.1:8080
yalnizca 127.0.0.1 dinleniyor — Ctrl+C ile dur
```

Tarayıcıda `http://127.0.0.1:8080` adresini açın, sonra `icerik/index.md`
dosyasını düzenleyip kaydedin; sunucu değişikliği fark eder ve şunu yazar:

```console
yeniden uretildi: 1 sayfa (korunan 2)
```

Tarayıcıdaki sayfa kendiliğinden yenilenir: servis edilen HTML'e enjekte edilen
betik, `/__siteturk/durum` adresine 500 ms aralıkla gider ve üretim sayacı
değiştiyse `location.reload()` çağırır.

> Bu makinede 8080 portu `llama-server` tarafından kullanıldığı için canlı
> denetim `--port 8137` ile yapıldı: `GET http://127.0.0.1:8137/` isteği
> `200` döndürdü ve gövdede yoklama betiği doğrulandı. Varsayılan port `8080`'dir.

### Adlandırma kuralı

```text
icerik/hakkimizda.md  ->  dist/hakkimizda/index.html  ->  /hakkimizda/
icerik/blog/neden.md  ->  dist/blog/neden/index.html  ->  /blog/neden/
icerik/index.md       ->  dist/index.html              ->  /
```

Ön bilgideki `adres` alanı verilirse bu kural geçersiz kılınır.

## Test

```console
$ cargo test
   ...
test result: ok. 174 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.21s
   ...
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
   ...
test result: ok. 23 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.26s
   ...
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.56s
```

**Toplam 205 test: 174 birim (kütüphane) + 7 birim (CLI kabuğu) + 23 entegrasyon
+ 1 doküman testi. Başarısız: 0.** Test paketi üst üste üç kez çalıştırıldı;
sunucu testi dâhil sonuçlar kararlıdır.

Kapsanan kenar durumları:

| Alan | Kapsanan durumlar |
|---|---|
| Markdown | ATX ve setext başlık, paragraf, sıralı/sırasız liste, **iç içe liste**, kapatmalı ve girintili kod bloğu, satır içi kod, bağlantı, görsel, kalın/italik, alıntı, yatay çizgi, tablo, otomatik bağlantı, geri kaçış |
| Markdown — bozuk girdi | **boş dosya**, kaçırılmamış ham HTML, kapanmayan bağlantı, kapanmayan vurgu, kapanmayan kod işareti, kapanmayan kod bloğu, 6. seviyeden derin başlık, blok derinliği sınırı, Türkçe başlık kimliği, tırnak/ampersand kaçışı |
| Ön bilgi | blok yok, tam blok, tek satırda liste, blok liste, tırnaklı değer, boş satır ve `#` yorumu, `taslak`/`siralama` tip denetimi, **kapanmayan blok**, **iki noktasız satır**, **tanımsız alan** |
| UTF-8 BOM | BOM'lu metin, BOM'suz metin, **ardışık çoklu BOM**, metin ortasındaki BOM'un korunması, yalnız BOM'dan oluşan metin |
| Şablon | değişken, koşul/alternatif, dizi ve harita döngüsü, iç içe döngü, **döngü sınırı**, **derinlik sınırı**, **tanımsız değişken**, kapanmayan işaret, eşleşmeyen kapanış, kapanmamış blok, bilinmeyen etiket/işlev/süzgeç, kaçış fonksiyonu, `| ham` süzgeci, `ham`/`varlik`/`kisalt` işlevleri, geçersiz yol, hata mesajında dosya ve satır |
| Üretim | tam site üretimi, **kısmi yeniden üretim**, **ölmüş dosya temizliği**, atomik yazma, taslak sayfaların üretilmemesi, `robots.txt` engellemesi, slug çakışması, varlık kopyalama/küçültme/bağlantı yeniden yazma, varlık temizlik uyarısı, uyarıların konsola yazılması, **bozuk yapılandırma**, **bozuk ön bilgi**, deterministik çıktı, UTF-8 dosya/klasör adları |
| Site haritası | temel adrese birleştirme, başta/sona ince olon, boş site, `lastmod` yazımı, adres çakışması, robots `Disallow` üretimi |
| Sunucu | HTTP istek ayrıştırma (uçtan uca dosya sunumu), **yol kaçışı reddi**, yalnız yerel adrese bağlanma, durum ucu ve betik enjeksiyonu |

Testlerde ağ erişimi yoktur; sunucu testleri `TcpListener::bind("127.0.0.1:0")`
ile geçici port alır, port numarası tahmin edilmez. Geçici dizin yardımcısı
kendi kodumuzdur (`tempfile` crate'i bağımlılık politikası gereği yasaktır) ve
`Drop` ile temizlenir; temizlik hatası bilinçli olarak yutulur çünkü `Drop`
içinden hata döndürülemez.

## Proje Yapısı

```text
14-siteturk/
├── Cargo.toml
├── Cargo.lock
├── LICENSE.txt
├── README.md
├── .gitignore
├── ornek/                     çalışan örnek site
│   ├── siteturk.json
│   ├── icerik/                index.md · hakkimizda.md · blog/neden.md · blog/taslak-notlar.md
│   ├── sablonlar/sayfa.html
│   └── static/sayfa.css
├── src/
│   ├── lib.rs                 çekirdek kütüphane; modülleri dışa açar
│   ├── main.rs                CLI kabuğu (new / build / serve / check)
│   ├── error.rs               Hata enum'u + Display + std::error::Error
│   ├── frontmatter.rs         ön bilgi ayrıştırma
│   ├── markdown.rs            CommonMark alt kümesi → HTML
│   ├── template.rs            sığ şablon dili
│   ├── discover.rs            içerik keşfi, adlandırma ve sıralama
│   ├── assets.rs              varlık kopyalama, imza, küçültme, bağlantı yeniden yazma
│   ├── sitemap.rs             sitemap.xml + robots.txt
│   ├── build.rs               üretim hattı, atomik yazma, kısmi yeniden üretim
│   ├── serve.rs               std::net::TcpListener tabanlı yerel canlı yenileme sunucusu
│   ├── ornek/                 `new` komutunun gömdüğü örnek içerikler (include_str!)
│   └── sablonlar/sayfa.html   `new` komutunun gömdüğü örnek şablon (include_str!)
└── tests/
    └── entegrasyon.rs         23 uçtan uca test
```

Modüller tek yönlü bir hat izler ve geriye bağımlılık kurmaz:
`frontmatter` → `markdown` → `template` → `build`. `serve` yalnızca `build`'i
çağırır; `main` yalnızca CLI kabuğudur.

## Yapılandırma

Yapılandırma tek dosyadır: proje kökündeki `siteturk.json`. **Dosya yoksa
varsayılanlar kullanılır**, hata üretilmez. Tanımsız alan adı hata üretir
(`deny_unknown_fields`), böylece sessizce yutulan bir ayar olamaz.

| Alan | Varsayılan | Etkisi |
|---|---|---|
| `kaynak` | `"icerik"` | Markdown içerik klasörü; özyinelemeli taranır. |
| `cikti` | `"dist"` | Üretilen sitenin yazılacağı klasör. |
| `sablon` | `"sablonlar/sayfa.html"` | Her sayfaya uygulanan HTML şablonu. |
| `varlik` | `"static"` | Kopyalanacak statik dosya klasörü; yoksa atlanır. |
| `adres` | `"https://ornek.example"` | Mutlak temel adres; `sitemap.xml` ve `robots.txt` bunu kullanır. |
| `sitemap` | `true` | `sitemap.xml` üretilsin mi. |
| `robots` | `true` | `robots.txt` üretilsin mi. |
| `taslaklari_yayinla` | `false` | `taslak: true` sayfalar da üretilsin mi. |
| `imzali_varlik` | `true` | Varlık adlarına içerik imzası (`.92d54d9ed42603c0.css`) eklensin mi. |
| `kucult` | `true` | Varlıklarda boşluk ve yorum kısaltma uygulansın mı. |

Depodaki `ornek/siteturk.json`:

```json
{
  "kaynak": "icerik",
  "cikti": "dist",
  "sablon": "sablonlar/sayfa.html",
  "varlik": "static",
  "adres": "https://ornek.example",
  "sitemap": true,
  "robots": true,
  "taslaklari_yayinla": false,
  "imzali_varlik": true,
  "kucult": true
}
```

### Komut satırı bayrakları

| Bayrak | Varsayılan | Etkisi |
|---|---|---|
| `-p, --proje <KLASOR>` | çalışma dizini | Üzerinde çalışılacak proje kökü. |
| `build --tam-uretim` | kapalı | Önbelleği yok say; her şeyi yeniden yaz. |
| `build -v, --ayrinti` | kapalı | Süreyi ve üretilen dosyaları tek tek yazdır. |
| `serve --port <PORT>` | `8080` | Dinlenecek port; **her zaman** `127.0.0.1`. |
| `serve --yoklama-ms <MS>` | `400` | **Dosya değişikliği** yoklama aralığı. Bağlantı yoklaması bundan bağımsız ve çok daha sıktır (20 ms), yoksa her istek bir tam dosya taramasını beklerdi. |

### Ön bilgi alanları

```text
---
baslik: Hakkımızda
tarih: 2026-09-29
ozet: Tek cümlelik özet.
yazar: web ekibi
etiketler:
  - tanitim
  - statik-site
siralama: 3
taslak: false
---
```

Kabul edilen alan adları yalnızca bunlardır: `baslik`, `tarih`, `etiketler`,
`ozet`, `yazar`, `taslak`, `slug`, `siralama`. `etiketler` hem blok liste
(`- oge`) hem tek satırda dizi (`[a, b]`) olarak yazılabilir. `siralama` tam
sayı olmalıdır ve küçükten büyüğe sıralama yapar; verilmezse dosya adına göre
sıralanır. `slug` verilirse dosya yolundan türetilen adres geçersiz kılınır
(başka bir sayfa aynı adresi üretiyorsa hata verilir).

### Şablon değişkenleri

| Değişken | İçerik |
|---|---|
| `site.adres` | Yapılandırmadaki mutlak adres. |
| `site.sayfaSayisi` | Yayımlanan sayfa sayısı (taslaklar hariç). |
| `site.varliklar` | Varlık adı → imzalı adres haritası. |
| `sayfa.baslik` · `sayfa.ozet` · `sayfa.yazar` · `sayfa.tarih` | Ön bilgi alanları. |
| `sayfa.taslak` | `true`/`false`. |
| `sayfa.etiketler` | Dizi; `{% for e in sayfa.etiketler %}` ile gezilir. |
| `sayfa.govde` | Markdown'ın HTML karşılığı; `| ham` ile yazılmalıdır. |
| `sayfa.adres` | Sayfanın kendi adresi (menü vurgusu için). |
| `tumSayfalar` | Yayımlanan tüm sayfaların listesi (`adres`, `baslik`, `ozet`, `tarih`). |

```html
<title>{{ sayfa.baslik }} — {{ site.adres }}</title>
{% for s in tumSayfalar %}<a href="{{ s.adres }}">{{ s.baslik }}</a>{% endfor %}
{{ sayfa.govde | ham }}
```

## Bilinen Sınırlamalar

- **Markdown'da desteklenmeyen sözdizimi yoktur — ama kapsam dardır.**
  Dipnotlar, görev listesi, tanım listesi, HTML gömme, bağlantı başlığı
  tanımları ve `~~üstü çizili~~` **yoktur**. Desteklenmeyen ya da hatalı
  sözdizimi sessizce yutulmaz: her durum için `satir N: ...` biçiminde uyarı
  üretilir ve `build` çıktısında listelenir. Ham HTML kaçışlanır (güvenlik
  gereği içeriği çalıştırılmaz), yalnızca uyarı üretilir.
- **Tablo desteği GFM uyumlu değildir.** Başlık satırı ve `| --- |` ayırıcı
  zorunludur; hizalama (`---:`) yok sayılır. Hücre içinde `|` kaçışı desteklenmez.
- **Sıfırdan yazılmış alt küme, tam CommonMark değildir.** Bağlantı tanımı
  (`[a]: url`), iç içe alıntı içinde liste gibi kenar durumlar kapsam dışıdır.
- **UTF-8 BOM desteklenir.** Windows düzenleyicileri ve PowerShell'in
  `Set-Content -Encoding utf8` kipi dosyalara BOM ekler; BOM taşıyan bir `.md`
  dosyasında ön bilgi bloğu aksi hâlde tanınmaz ve dosya başlıksız bir sayfaya
  dönüşür. Üretici, okunan her metnin başındaki BOM'ları kaldırır; BOM hem
  `.md` hem `siteturk.json` hem şablon dosyalarında güvenle yutulur.
- **Sığ şablon dilinde kasıtlı olarak eksik olanlar:** kullanıcı tanımlı
  fonksiyon, dosya çağırma (`include`), aritmetik, süzgeç zinciri, makro ve
  şablon eklentileri. Yalnızca `ham`, `varlik` ve `kisalt` işlevleri vardır.
- **Ön yüz varlık boru hattı yoktur.** Sass, Less, PostCSS, TypeScript, SVG
  optimizasyonu veya ikon üretimi yapılmaz; `static/` klasörü bayt bayt kopyalanır.
- **Akış çıktısı yoktur.** Her şey önce bellekte üretilir, sonra tek seferde
  diske yazılır; 32 MiB üstü tek sayfa üretilemez (`MAX_CIKTI_BAYT`).
- **Canlı yenileme sunucusu geliştirme içindir.** Tek iş parçacıklıdır, bağlantı
  başına tek istek işler, `GET`/`HEAD` dışında yöntem desteklemez ve yalnızca
  `127.0.0.1` üzerinde dinler. Kimlik doğrulama, TLS ve eşzamanlılık yoktur;
  üretim sunucusu için **kullanılmamalıdır**. `accept` engellemeyen kipte
  çalışır (aksi hâlde tarayıcı kapalıyken sunucu körleşir ve dosya değişikliklerini
  hiç fark etmezdi); kabul edilen bağlantının okuma süresi 2 saniyeyle sınırlıdır,
  aksi hâlde açık bırakılmış bir bağlantı döngüyü kilitlerdi. Tarayıcı
  tarafındaki yenileme yoklaması 500 ms'de bir `/__siteturk/durum` adresine gider.
- **Kısmi yeniden üretim `mtime` + boyuta dayanır.** Dosya sistemi çözünürlüğü
  veya saat kayması önbelleği geçersiz kılabilir. Şüphede `--tam-uretim`
  kullanın.
- **Çıktı klasörü yalnızca kendi ürettiği dosyaları temizler.** Önbellekte
  kayıtlı olmayan bir dosyaya dokunulmaz; elle eklenen dosyalar yerinde kalır.
- **Sitemap'te `changefreq`/`priority` üretilmez**, yalnızca `loc` ve `lastmod`.
- **Markdown alt kümesi 21 NoMarka ile örtüşür.** Bilinçli olarak kopyalanmıştır;
  her kopya kendi testine sahiptir (bkz. `## Atıflar`).
- **Derleyici testleri `Drop` içinde sessizce yutulan temizlik hatası dışında**
  hiçbir yerde hata yutmaz; üretim kodunda `unwrap`, `expect` ve `panic!` yoktur.

## Gelecek Geliştirmeler

- Markdown'a sıfırdan yazılmış kalan blok yapıları: görev listesi, tanım listesi,
  dipnotlar.
- Tablolarda hizalama sütunları ve hücre içi `|` kaçışı.
- Şablon dilinde `{% include %}` ve şablon eklenti mekanizması.
- Akış (streaming) üretim ile `MAX_CIKTI_BAYT` sınırının kaldırılması.
- Ön yüz boru hattı: en azından CSS/SVG küçültme ve dosya birleştirme.
- Varlık önbelleği için içerik imzası yerine daha güçlü ama yine bağımlılıksız
  bir karma (FNV-1a gibi).
- Çok dilli site yapılandırması (`--lang` ve klasör başına dil).
- Sunucu için eşzamanlı bağlantı işleme (yine `std::net` ile, `unsafe` olmadan).

## Troubleshooting

**1. `hata: on bilgi hatasi (a.md:3): tanimsiz alan \`aciklama\`; kabul edilenler: baslik, tarih, etiketler, ozet, yazar, taslak, slug, siralama`**

*Belirti:* Üretim durur; hata dosya adını ve satır numarasını verir.
*Neden:* Ön bilgide kabul edilmeyen bir alan adı var.
*Çözüm:* Alan adını kabul edilen sekiz addan biriyle değiştirin ya da alanı
gövdeye taşıyın. Yazım hatası en sık neden: `aciklama` yerine `ozet`,
`date` yerine `tarih`, `tags` yerine `etiketler`, `author` yerine `yazar`.

**2. `hata: yapilandirma hatasi (...siteturk.json): expected value at line 1 column 1`**

*Belirti:* Yapılandırma okunmuyor, üretim hiç başlamıyor.
*Neden:* JSON söz dizimi hatalı (en sık neden sondaki fazladan virgül), ya da
dosya boş.
*Çözüm:* Virgülleri ve tırnakları denetleyin. Şüphede dosyayı geçici olarak
silin: yapılandırma dosyası yoksa üretici hata vermez, varsayılanlarla çalışır.
Böylece sorunun yapılandırmada mı içerikte mi olduğunu ayırabilirsiniz.

**3. `hata: kullanim hatasi: sablon dosyasi bulunamadi: .../sablonlar/sayfa.html (yapilandirmada \`sablon\`)`**

*Belirti:* Üretim başlamadan durur.
*Neden:* `siteturk.json` içindeki `sablon` yolu yanlış, ya da dosya gerçekten yok.
*Çözüm:* Yolu yapılandırma dosyasına **göreli** ve `/` ile yazın
(`"sablon": "sablonlar/sayfa.html"`). `siteturk new` ile üretilen iskelet bu
dosyayı zaten içerir. `siteturk check` aynı hatayı üretmeden önce söyler.

**4. Sunucu açılıyor ama sayfa yenilenmiyor**

*Belirti:* `siteturk serve` dinlendiğini yazıyor, sayfa açılıyor, dosyayı
değiştirdiğiniz hâlde yenileme olmuyor.
*Neden:* Üretim sayacı değişmediği için betik `location.reload()` çağırmıyor —
değiştirdiğiniz dosya `icerik/` altında değildir (örneğin `sablonlar/` veya
`static/` içinde), ya da sekme o anda sunucuya bağlı değildir.
*Çözüm:* Değişikliği `icerik/` altında yapın. `--yoklama-ms` değerini düşürerek
(örneğin `200`) gecikmeyi azaltın. Sunucu dosya değişikliğini tarayıcı bağlı
olmadan da algılar; yine de işe yaramazsa `Ctrl+C` ile durdurup
`siteturk build` çalıştırın.

**5. `siteturk serve` "127.0.0.1:8080 adresine baglanilamadi" hatası veriyor**

*Belirti:* Sunucu açılmıyor, üretim yapılmış.
*Neden:* 8080 portu başka bir program tarafından işgal edilmiş.
*Çözüm:* Başka bir port verin: `siteturk serve --port 8137`. (Bu depoda canlı
denetim sırasında 8080'in `llama-server` tarafından kullanıldığı görüldü ve
denetim `--port 8137` ile yapıldı.)

**6. Sayfa üretildi ama tarayıcıda eski hâli görünüyor**

*Belirti:* İçerik değişti, `dist/` güncel, tarayıcı eski sürümü gösteriyor.
*Neden:* Tarayıcı önbelleği. `imzali_varlik: true` ise CSS dosya adı imzalıdır
(`sayfa.92d54d9ed42603c0.css`) ve bu sorun varlıklarda yaşanmaz; sorun yalnız
HTML'de olur.
*Çözüm:* `siteturk build --tam-uretim` ile önbelleği atlayın, tarayıcıda
`Ctrl+F5` ile zorla yenileyin.

**7. `linker 'link.exe' not found` veya derleme ortasında kesiliyor**

*Belirti:* `cargo build` bağlantı aşamasında hata veriyor.
*Neden:* MinGW / Rust GNU araç zinciri `PATH` üzerinde değil.
*Çözüm:* Terminale şunu ekleyip `cargo`yu yeniden çalıştırın:

```console
$env:PATH = "%USERPROFILE%\.cargo\bin;%USERPROFILE%\AppData\Local\Microsoft\WinGet\Packages\BrechtSanders.WinLibs.POSIX.UCRT_Microsoft.Winget.Source_8wekyb3d8bbwe\mingw64\bin;" + $env:PATH
```

**8. Sayfa başlığı boş geliyor, dosyada `baslik` yazılı ama görünmüyor**

*Belirti:* Şablon `{{ sayfa.baslik }}` yazdırıyor, çıktı boş; Markdown başlığı da
görünmüyor.
*Neden:* Dosyanın başında `---` yoktur, ön bilgi bloğu tanınmamıştır; ya da dosya
BOM ile başlıyordu.
*Çözüm:* `---` çizgilerinin dosyanın **en üstünde** olduğundan emin olun (Markdown
içindeki `---` ile karışmasın). BOM artık otomatik olarak yutulur, ama elle
kontrol etmek sorunu göstermek için hızlı bir yoldur: dosyayı Notepad'de "UTF-8
(BOM'suz)" olarak kaydedin. Tanımsız bir alan adı kullanıyorsanız hata mesajı
hangi satırın bozuk olduğunu doğrudan söyler.

## Atıflar

Bu proje hiçbir harici kütüphane **kodu** kopyalamaz; aşağıdakiler tasarım
dayanaklarıdır.

- **CommonMark söz dizimi kutusu** — desteklenen alt kümenin tanımı.
  <https://spec.commonmark.org/>
- **Görsel biçimlendirme (GFM) tabloları** — tablo alt kümesi.
  <https://github.github.com/gfm/>
- **Sitemap protokolü** — `sitemap.xml` biçiminin kaynağı.
  <https://www.sitemaps.org/protocol.html>
- **RFC 9110 — HTTP Semantics** — sunucunun istek/yanıt ayrıştırması.
  <https://www.rfc-editor.org/rfc/rfc9110>
- **RFC 3986 — URI Generic Syntax** — yüzde kaçışlarının çözülmesi ve
  yol kaçışı denetimi. <https://www.rfc-editor.org/rfc/rfc3986>
- **Rust standart kütüphane belgeleri** — `std::net::TcpListener`, `std::fs`,
  `std::time`. <https://doc.rust-lang.org/std/>
- **Rust 2021 edition rehberi** — <https://doc.rust-lang.org/edition-guide/edition-2021/>
- **serde** ve **serde_json** — yapılandırma şeması ve önbellek.
  <https://serde.rs/> · <https://github.com/serde-rs/json>
- **clap** — CLI argüman ayrıştırma. <https://docs.rs/clap/>
- **21 NoMarka ile bilinçli örtüşme** — Markdown ayrıştırıcısı mantığı bu
  projeye kopyalanmıştır (WORKER_CONTRACT § 9). Her kopya kendi test
  vektörlerine sahiptir; ortak crate bu turun kapsamı dışındadır.
- **Tasarım kaynağı (yerel dosya, URL değildir):**
  `%USERPROFILE%\Desktop\Fikirler\14-statik-usta-ssg.html` — "StatikUsta"
  fikrinin ayrıntılı raporu. Markdown alt kümesi, şablon dili ve sunucu
  kipi bu raporun gereksinimlerine göre seçilmiştir.

## Üretim Atfı

Bu depo **OpenCode** ajanı tarafından, **`space-bunny-free`** modeli
(`opencode/space-bunny-free`) kullanılarak üretilmiştir.

- **Arac:** OpenCode
- **Model:** `opencode/space-bunny-free` (Space Bunny Free)
- **Tür:** Rust, `cargo build` / `cargo test` ile üretilmiş ve doğrulanmıştır.

Kaynak kod, testler ve dokümantasyon bu model tarafından yazılmıştır. İnsan
katkısı: gereksinim tanımı, kabul ölçütleri ve son kontroller.

## Lisans

MIT. Tam metin `LICENSE.txt` dosyasındadır. Bu proje dışarıdan derlenmiş hiçbir
kütüphaneyi **birlikte dağıtmaz**; `serde`, `serde_json` ve `clap` yalnızca
derleme bağımlılığıdır ve Rust'un kendi lisanslarıyla dağıtılır.
