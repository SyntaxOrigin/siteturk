//! CommonMark alt kümesi ayrıştırıcısı ve HTML üreticisi.
//!
//! Tasarım ilkesi **ağaçsız tek geçişli** üretimdir: ara bir belge ağacı kurulmaz,
//! HTML doğrudan bir `String` tamponuna yazılır (rapor § 07, "Veri modeli").
//!
//! Desteklenen blok yapıları: başlık (ATX ve setext), paragraf, sıralı/sırasız
//! liste (iç içe), alıntı, kod bloğu (kapatmalı ve girintili), yatay çizgi, tablo.
//! Desteklenen satır içi yapılar: satır içi kod, bağlantı, görsel, kalın, italik,
//! otomatik bağlantı, geri kaçış (`\`).
//!
//! Desteklenmeyen sözdizimi **sessizce yutulmaz**: her durum için
//! [`Uyari`] üretilir. Sessizce bozuk çıktı, bu araçta en kötü hatadır.

use std::collections::BTreeMap;
use std::fmt;

/// Üretilen uyarının tek satırlık, insan tarafından okunabilir açıklaması.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Uyari {
    /// Uyarının kaynaklandığı satır numarası (1 tabanlı).
    pub satir: usize,
    /// Uyarı metni.
    pub mesaj: String,
}

impl fmt::Display for Uyari {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "satir {}: {}", self.satir, self.mesaj)
    }
}

/// Bir Markdown belgesinin HTML çıktısı ve uyarı listesi.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkdownCikti {
    /// Üretilen HTML parçası.
    pub html: String,
    /// Ayrıştırma sırasında üretilen uyarılar.
    pub uyarilar: Vec<Uyari>,
}

/// Aynı satırda üretilebilecek en çok uyarı sayısı.
///
/// Sınırsız uyarı üretimi, binlerce satırlık bozuk dosyada günlüğü okunamaz
/// hale getirirdi; sınır bir hata yutma değil, çıktı boyutu denetimidir.
const EN_FAZLA_UYARI: usize = 100;

/// Tek geçişli ayrıştırıcının çalışma durumu.
struct Ayiklayici {
    cikti: String,
    uyarilar: Vec<Uyari>,
    baslik_sayaclari: BTreeMap<String, usize>,
    derinlik: usize,
}

/// Blok iç içe geçiş sınırı (alıntı ve liste yuvaları).
const MAX_BLOK_DERINLIGI: usize = 32;

/// Markdown belgesini HTML'e çevirir.
///
/// # Örnek
///
/// ```
/// use siteturk::markdown::ayikla;
///
/// let cikti = ayikla("# Baslik\n\nBir **kalin** metin.\n");
/// assert!(cikti.html.contains("<h1"));
/// assert!(cikti.uyarilar.is_empty());
/// ```
pub fn ayikla(kaynak: &str) -> MarkdownCikti {
    let mut ayiklayici = Ayiklayici {
        cikti: String::new(),
        uyarilar: Vec::new(),
        baslik_sayaclari: BTreeMap::new(),
        derinlik: 0,
    };
    let satirlar: Vec<&str> = kaynak.lines().collect();
    ayiklayici.bloklari_isle(&satirlar, 1);
    MarkdownCikti {
        html: ayiklayici.cikti,
        uyarilar: ayiklayici.uyarilar,
    }
}

/// Markdown belgesini HTML'e çevirir, uyarıları yok sayar.
pub fn ayikla_sessiz(kaynak: &str) -> String {
    ayikla(kaynak).html
}

impl Ayiklayici {
    /// Uyarı ekler; sınır aşıldıysa eklemez.
    fn uyari_ver(&mut self, satir: usize, mesaj: impl Into<String>) {
        if self.uyarilar.len() < EN_FAZLA_UYARI {
            self.uyarilar.push(Uyari {
                satir,
                mesaj: mesaj.into(),
            });
        }
    }

    /// Satır listesini blok düzeyinde işler.
    fn bloklari_isle(&mut self, satirlar: &[&str], taban: usize) {
        self.bloklari_isle_ile(satirlar, taban, true)
    }

    /// Satır listesini blok düzeyinde işler.
    ///
    /// `sargi` `false` iken paragraflar `<p>` etiketi olmadan yazılır. Bu, sıkı
    /// (tight) liste öğelerinin doğru görünmesi için gereklidir: `- bir` öğesi
    /// `<li>bir</li>` olmalıdır, `<li><p>bir</p></li>` değil.
    fn bloklari_isle_ile(&mut self, satirlar: &[&str], taban: usize, sargi: bool) {
        if self.derinlik >= MAX_BLOK_DERINLIGI {
            self.uyari_ver(
                taban,
                format!("blok ic ice gecis siniri ({MAX_BLOK_DERINLIGI}) asildi, icerik kesildi"),
            );
            return;
        }
        self.derinlik += 1;
        let mut i = 0usize;
        while i < satirlar.len() {
            let satir = satirlar[i];
            let kirp = satir.trim();

            if kirp.is_empty() {
                i += 1;
                continue;
            }

            if kapatma_cesididir(kirp) {
                i = self.kod_blogu_isle(satirlar, i, taban);
                continue;
            }

            if yatay_cizgidir(kirp) {
                self.cikti.push_str("<hr />\n");
                i += 1;
                continue;
            }

            if let Some(seviye) = atx_basligi(kirp) {
                self.baslik_isle(seviye, kirp, taban + i);
                i += 1;
                continue;
            }

            if kirp.starts_with('>') {
                i = self.alinti_isle(satirlar, i, taban);
                continue;
            }

            if liste_isaretidir(kirp) {
                i = self.liste_isle(satirlar, i, taban);
                continue;
            }

            if girintili_kod_satiridir(satir) {
                i = self.girintili_kod_isle(satirlar, i);
                continue;
            }

            if tablo_basligi_mi(satirlar, i) {
                i = self.tablo_isle(satirlar, i, taban);
                continue;
            }

            i = self.paragraf_isle(satirlar, i, taban, sargi);
        }
        self.derinlik -= 1;
    }

    /// Kapatmalı (fenced) kod bloğunu işler ve kapanış satırının ardından döner.
    fn kod_blogu_isle(&mut self, satirlar: &[&str], baslangic: usize, taban: usize) -> usize {
        let (delil, adet, bilgi) = kapatma_ayikla(satirlar[baslangic].trim());
        let mut govde = String::new();
        let mut j = baslangic + 1;
        let kapali = loop {
            match satirlar.get(j) {
                None => break false,
                Some(satir) => {
                    let kirp = satir.trim();
                    let yeterli = kirp.len() >= adet
                        && kirp.chars().take_while(|c| *c == delil).count() >= adet;
                    if kirp.starts_with(delil) && yeterli {
                        break true;
                    }
                    govde.push_str(satir);
                    govde.push('\n');
                    j += 1;
                }
            }
        };
        if !kapali {
            self.uyari_ver(
                taban + baslangic,
                "kapanmayan kod blogu; dosya sonuna kadar icerik kod sayildi",
            );
        }
        let sinif = dil_sinifi(&bilgi);
        self.cikti.push_str("<pre><code");
        if !sinif.is_empty() {
            self.cikti.push_str(" class=\"");
            self.cikti.push_str(&sinif);
            self.cikti.push('"');
        }
        self.cikti.push('>');
        self.cikti.push_str(&kacis(&govde));
        self.cikti.push_str("</code></pre>\n");
        if kapali {
            j + 1
        } else {
            j
        }
    }

    /// Girintili (dört boşluk) kod bloğunu işler.
    fn girintili_kod_isle(&mut self, satirlar: &[&str], baslangic: usize) -> usize {
        let mut govde = String::new();
        let mut i = baslangic;
        while i < satirlar.len() {
            let satir = satirlar[i];
            if satir.trim().is_empty() {
                // Ardından kod satırı geliyorsa boşluk korunur.
                let sonraki_kod = satirlar
                    .get(i + 1)
                    .map(|s| girintili_kod_satiridir(s))
                    .unwrap_or(false);
                if !sonraki_kod {
                    break;
                }
                govde.push('\n');
                i += 1;
                continue;
            }
            if !girintili_kod_satiridir(satir) {
                break;
            }
            govde.push_str(&satir[4..]);
            govde.push('\n');
            i += 1;
        }
        self.cikti.push_str("<pre><code>");
        self.cikti.push_str(&kacis(&govde));
        self.cikti.push_str("</code></pre>\n");
        i
    }

    /// ATX başlığını (`# Baslik`) işler ve `id` niteliği üretir.
    fn baslik_isle(&mut self, seviye: usize, satir: &str, numara: usize) {
        let ham = &satir[seviye..];
        let ham = ham.trim_start();
        let icerik = ham.trim_end_matches('#').trim_end();
        let kimlik = self.baslik_kimligi(icerik);
        let etiket = format!("h{seviye}");
        self.cikti.push('<');
        self.cikti.push_str(&etiket);
        self.cikti.push_str(" id=\"");
        self.cikti.push_str(&kacis_nitelik(&kimlik));
        self.cikti.push_str("\">");
        self.satiri_isle(icerik, numara);
        self.cikti.push_str("</");
        self.cikti.push_str(&etiket);
        self.cikti.push_str(">\n");
    }

    /// Başlık metninden URL çapası üretir; yinelenen kimlikler `-2`, `-3` alır.
    ///
    /// Harfler küçültülür ama **silinmez**: "Nasıl çalışır?" başlığı
    /// `nasıl-çalışır` olur. HTML5 kimlikleri Unicode'e izin verir ve sitenin
    /// dosya adları da Türkçe karakteri korur; `k-harf-8` dönüşümü okunabilirliği
    /// bozardı.
    fn baslik_kimligi(&mut self, metin: &str) -> String {
        let mut temel = String::new();
        let mut onceki_tire = false;
        for karakter in metin.chars() {
            if karakter.is_alphanumeric() {
                for kucuk in karakter.to_lowercase() {
                    temel.push(kucuk);
                }
                onceki_tire = false;
            } else if karakter == '_' || karakter == '-' {
                temel.push(karakter);
                onceki_tire = false;
            } else if !onceki_tire && !temel.is_empty() {
                temel.push('-');
                onceki_tire = true;
            }
        }
        while temel.ends_with('-') {
            temel.pop();
        }
        if temel.is_empty() {
            temel.push_str("bolum");
        }
        let sayac = self.baslik_sayaclari.entry(temel.clone()).or_insert(0);
        *sayac += 1;
        if *sayac == 1 {
            temel
        } else {
            format!("{}-{}", temel, sayac)
        }
    }

    /// Alıntı bloğunu işler; `>` önekleri soyulur ve içerik yeniden ayrıştırılır.
    fn alinti_isle(&mut self, satirlar: &[&str], baslangic: usize, taban: usize) -> usize {
        let mut icerik: Vec<String> = Vec::new();
        let mut i = baslangic;
        while i < satirlar.len() {
            let satir = satirlar[i];
            if satir.trim_start().starts_with('>') {
                let soyulmus = satir.trim_start();
                let kalan = soyulmus[1..].strip_prefix(' ').unwrap_or(&soyulmus[1..]);
                icerik.push(kalan.to_string());
                i += 1;
            } else if satir.trim().is_empty() {
                break;
            } else if !icerik.is_empty() {
                // Alıntı bloğu, kesintisiz paragraf devamı olarak sürer.
                icerik.push(satir.to_string());
                i += 1;
            } else {
                break;
            }
        }
        self.cikti.push_str("<blockquote>\n");
        let odunc: Vec<&str> = icerik.iter().map(|s| s.as_str()).collect();
        self.bloklari_isle(&odunc, taban + baslangic);
        self.cikti.push_str("</blockquote>\n");
        i
    }

    /// Sıralı veya sırasız listeyi işler; iç içe listeler girintiden türetilir.
    fn liste_isle(&mut self, satirlar: &[&str], baslangic: usize, taban: usize) -> usize {
        let ilk_isaret = liste_isareti(satirlar[baslangic].trim_start());
        let sirali = ilk_isaret.map(|i| i.sirali).unwrap_or(false);
        let etiket = if sirali { "ol" } else { "ul" };

        self.cikti.push('<');
        self.cikti.push_str(etiket);
        self.cikti.push_str(">\n");

        let mut i = baslangic;
        let mut gevsek = false;
        while i < satirlar.len() {
            let satir = satirlar[i];
            let kirp = satir.trim_start();
            if kirp.is_empty() {
                // İki öğe arasındaki boş satır listede gevşeklik yaratır.
                let sonraki_gevsek = satirlar
                    .get(i + 1)
                    .and_then(|s| liste_isareti(s.trim_start()))
                    .map(|isaret| isaret.sirali == sirali)
                    .unwrap_or(false);
                if sonraki_gevsek {
                    gevsek = true;
                    i += 1;
                    continue;
                }
                break;
            }

            let girinti = girinti_genisligi(satir);
            let isaret = match liste_isareti(kirp) {
                Some(isaret) if isaret.sirali == sirali && girinti < isaret.icerik_genislik => {
                    isaret
                }
                _ => break,
            };

            let icerik_genislik = girinti + isaret.icerik_genislik;
            let mut oge: Vec<String> = Vec::new();
            let ilk_satir = kirp[isaret.uzunluk..].trim_start().to_string();
            oge.push(ilk_satir);
            i += 1;

            while i < satirlar.len() {
                let sonraki = satirlar[i];
                if sonraki.trim().is_empty() {
                    let ardindan = satirlar.get(i + 1).copied();
                    match ardindan {
                        Some(s)
                            if !s.trim().is_empty() && girinti_genisligi(s) >= icerik_genislik =>
                        {
                            gevsek = true;
                            oge.push(String::new());
                            i += 1;
                        }
                        _ => break,
                    }
                    continue;
                }
                if girinti_genisligi(sonraki) < icerik_genislik {
                    break;
                }
                let soyulmus = &sonraki[icerik_genislik..];
                oge.push(soyulmus.to_string());
                i += 1;
            }

            self.cikti.push_str("<li>");
            let odunc: Vec<&str> = oge.iter().map(|s| s.as_str()).collect();
            self.bloklari_isle_ile(&odunc, taban + baslangic, gevsek);
            self.cikti.push_str("</li>\n");
        }

        self.cikti.push_str("</");
        self.cikti.push_str(etiket);
        self.cikti.push_str(">\n");
        i
    }

    /// GFM tarzı tabloyu işler.
    fn tablo_isle(&mut self, satirlar: &[&str], baslangic: usize, taban: usize) -> usize {
        let baslik_satiri = satirlar[baslangic];
        let ayirici = hizalama_satiri(satirlar[baslangic + 1]);
        let hizalamalar = match ayirici {
            Some(h) => h,
            None => {
                self.uyari_ver(
                    taban + baslangic,
                    "`|` iceren satir tablo olarak baslamiyor; ayirici satir eksik, paragraf olarak yorumlandi",
                );
                return self.paragraf_isle(satirlar, baslangic, taban, true);
            }
        };

        let basliklar = tablo_hucreleri(baslik_satiri);
        self.cikti.push_str("<table>\n<thead>\n<tr>");
        for (indeks, hucre) in basliklar.iter().enumerate() {
            self.cikti.push_str("<th");
            if let Some(h) = hizalamalar.get(indeks).and_then(|h| h.as_ref()) {
                self.cikti.push_str(h);
            }
            self.cikti.push('>');
            self.satiri_isle(hucre, taban + baslangic);
            self.cikti.push_str("</th>\n");
        }
        self.cikti.push_str("</tr>\n</thead>\n<tbody>\n");

        let mut i = baslangic + 2;
        while i < satirlar.len() {
            if satirlar[i].trim().is_empty() || !satirlar[i].contains('|') {
                break;
            }
            let hucreler = tablo_hucreleri(satirlar[i]);
            self.cikti.push_str("<tr>");
            for indeks in 0..basliklar.len() {
                self.cikti.push_str("<td");
                if let Some(h) = hizalamalar.get(indeks).and_then(|h| h.as_ref()) {
                    self.cikti.push_str(h);
                }
                self.cikti.push('>');
                if let Some(hucre) = hucreler.get(indeks) {
                    self.satiri_isle(hucre, taban + i);
                }
                self.cikti.push_str("</td>\n");
            }
            if hucreler.len() != basliklar.len() {
                self.uyari_ver(
                    taban + i,
                    format!(
                        "tablo satiri {} hucre, baslikta {} hucre var; eksik hucreler bos birakildi",
                        hucreler.len(),
                        basliklar.len()
                    ),
                );
            }
            self.cikti.push_str("</tr>\n");
            i += 1;
        }
        self.cikti.push_str("</tbody>\n</table>\n");
        i
    }

    /// Paragrafı (ve varsa setext başlığını) işler.
    fn paragraf_isle(
        &mut self,
        satirlar: &[&str],
        baslangic: usize,
        taban: usize,
        sargi: bool,
    ) -> usize {
        let mut satirlar_listesi: Vec<&str> = Vec::new();
        let mut i = baslangic;
        let mut setext_seviye = 0usize;
        while i < satirlar.len() {
            let satir = satirlar[i];
            let kirp = satir.trim();
            if kirp.is_empty() {
                break;
            }
            if !satirlar_listesi.is_empty() {
                if let Some(seviye) = setext_alt_cizgi(kirp) {
                    setext_seviye = seviye;
                    i += 1;
                    break;
                }
                if atx_basligi(kirp).is_some()
                    || kapatma_cesididir(kirp)
                    || yatay_cizgidir(kirp)
                    || kirp.starts_with('>')
                    || liste_isaretidir(kirp)
                {
                    break;
                }
            }
            satirlar_listesi.push(satir);
            i += 1;
        }

        if setext_seviye > 0 && !satirlar_listesi.is_empty() {
            let metin = satirlar_listesi.join(" ");
            let etiket = format!("h{setext_seviye}");
            let kimlik = self.baslik_kimligi(metin.trim());
            self.cikti.push('<');
            self.cikti.push_str(&etiket);
            self.cikti.push_str(" id=\"");
            self.cikti.push_str(&kacis_nitelik(&kimlik));
            self.cikti.push_str("\">");
            self.satiri_isle(metin.trim(), taban + baslangic);
            self.cikti.push_str("</");
            self.cikti.push_str(&etiket);
            self.cikti.push_str(">\n");
            return i;
        }

        if satirlar_listesi.is_empty() {
            // Tanınmayan bir blok başlangıcı: satırı atlayıp sonsuz döngüyü önle.
            return baslangic + 1;
        }

        if sargi {
            self.cikti.push_str("<p>");
        }
        for (indeks, satir) in satirlar_listesi.iter().enumerate() {
            if indeks > 0 {
                self.cikti.push('\n');
            }
            self.satiri_isle(satir.trim(), taban + baslangic + indeks);
        }
        if sargi {
            self.cikti.push_str("</p>\n");
        }
        i
    }

    /// Satır içi işaretlemeyi çözer ve HTML'i doğrudan tampona yazar.
    fn satiri_isle(&mut self, girdi: &str, satir: usize) {
        let karakterler: Vec<char> = girdi.chars().collect();
        let mut tampon = String::new();
        let mut i = 0usize;
        while i < karakterler.len() {
            let karakter = karakterler[i];
            match karakter {
                '\\' if i + 1 < karakterler.len() => {
                    tampon.push(karakterler[i + 1]);
                    i += 2;
                }
                '`' => {
                    let (acik, son) = backticks_adet(&karakterler, i);
                    match kapat_backtick(&karakterler, son, acik) {
                        Some(kapanma) => {
                            let icerik: String = karakterler[son..kapanma].iter().collect();
                            let icerik = icerik
                                .strip_prefix(' ')
                                .and_then(|s| s.strip_suffix(' '))
                                .unwrap_or(&icerik)
                                .to_string();
                            tampon.push_str("<code>");
                            tampon.push_str(&kacis(&icerik));
                            tampon.push_str("</code>");
                            i = kapanma + acik;
                        }
                        None => {
                            self.uyari_ver(
                                satir,
                                "kapanmayan satir ici kod isareti; kalan metin duz yazildi",
                            );
                            tampon.push('`');
                            i += 1;
                        }
                    }
                }
                '*' | '_' => {
                    let kosul_gecerli = karakter == '*'
                        || !onceki_kelime(&karakterler, i)
                        || karakterler
                            .get(i + 1)
                            .map(|c| c.is_whitespace())
                            .unwrap_or(true);
                    if !kosul_gecerli {
                        tampon.push(karakter);
                        i += 1;
                        continue;
                    }
                    let genislik = if karakterler.get(i + 1) == Some(&karakter) {
                        2
                    } else {
                        1
                    };
                    match kapat_vurgu(&karakterler, i, karakter, genislik) {
                        Some(kapanma) => {
                            let icerik: String =
                                karakterler[i + genislik..kapanma].iter().collect();
                            let etiket = if genislik == 2 { "strong" } else { "em" };
                            tampon.push('<');
                            tampon.push_str(etiket);
                            tampon.push('>');
                            self.gecici_satir(&mut tampon, &icerik, satir);
                            tampon.push_str("</");
                            tampon.push_str(etiket);
                            tampon.push('>');
                            i = kapanma + genislik;
                        }
                        None => {
                            self.uyari_ver(
                                satir,
                                format!("kapanmayan {} vurgusu; isaret duz yazildi", karakter),
                            );
                            tampon.push(karakter);
                            i += 1;
                        }
                    }
                }
                '!' if karakterler.get(i + 1) == Some(&'[') => {
                    match baglanti_coz(&karakterler, i + 1) {
                        Some((metin, adres, son)) => {
                            tampon.push_str("<img src=\"");
                            tampon.push_str(&kacis_nitelik(&adres));
                            tampon.push_str("\" alt=\"");
                            tampon.push_str(&kacis_nitelik(metin.trim()));
                            tampon.push_str("\" />");
                            i = son;
                        }
                        None => {
                            self.uyari_ver(
                                satir,
                                "kapanmayan gorsel baglantisi; metin duz yazildi",
                            );
                            tampon.push('!');
                            i += 1;
                        }
                    }
                }
                '[' => match baglanti_coz(&karakterler, i) {
                    Some((metin, adres, son)) => {
                        tampon.push_str("<a href=\"");
                        tampon.push_str(&kacis_nitelik(&adres));
                        tampon.push_str("\">");
                        self.gecici_satir(&mut tampon, &metin, satir);
                        tampon.push_str("</a>");
                        i = son;
                    }
                    None => {
                        self.uyari_ver(satir, "kapanmayan baglanti; metin duz yazildi");
                        tampon.push('[');
                        i += 1;
                    }
                },
                '<' => {
                    match otomatik_baglanti_coz(&karakterler, i) {
                        Some((adres, son)) => {
                            tampon.push_str("<a href=\"");
                            tampon.push_str(&kacis_nitelik(&adres));
                            tampon.push_str("\">");
                            tampon.push_str(&kacis(&adres));
                            tampon.push_str("</a>");
                            i = son;
                        }
                        None => {
                            // Ham HTML desteklenmiyor: kaçışlanır ve uyarılır.
                            if karakterler
                                .get(i + 1)
                                .map(|c| c.is_ascii_alphabetic() || *c == '/')
                                .unwrap_or(false)
                            {
                                self.uyari_ver(
                                    satir,
                                    "ham HTML desteklenmiyor; metin olarak kacislandi",
                                );
                            }
                            tampon.push_str("&lt;");
                            i += 1;
                        }
                    }
                }
                '&' => {
                    let parca: String = karakterler[i..(i + 12).min(karakterler.len())]
                        .iter()
                        .collect();
                    if parca.starts_with("&amp;")
                        || parca.starts_with("&lt;")
                        || parca.starts_with("&gt;")
                        || parca.starts_with("&quot;")
                        || parca.starts_with("&#")
                    {
                        tampon.push('&');
                    } else {
                        tampon.push_str("&amp;");
                    }
                    i += 1;
                }
                '>' => {
                    tampon.push_str("&gt;");
                    i += 1;
                }
                '"' => {
                    tampon.push_str("&quot;");
                    i += 1;
                }
                _ => {
                    tampon.push(karakter);
                    i += 1;
                }
            }
        }
        self.cikti.push_str(&tampon);
    }

    /// Vurgu içeriğini geçici tampona çözer (özyinelemeli satır içi ayrıştırma).
    fn gecici_satir(&mut self, tampon: &mut String, metin: &str, satir: usize) {
        let kaydedilen = std::mem::take(&mut self.cikti);
        self.satiri_isle(metin, satir);
        let uretilen = std::mem::replace(&mut self.cikti, kaydedilen);
        tampon.push_str(&uretilen);
    }
}

/// Bir liste işaretinin ayrıştırılmış hâli.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ListeIsareti {
    sirali: bool,
    uzunluk: usize,
    icerik_genislik: usize,
}

/// Bir satırın liste işaretiyle başlayıp başlamadığını bildirir.
fn liste_isaretidir(kirp: &str) -> bool {
    liste_isareti(kirp).is_some()
}

/// Baştaki liste işaretini ayrıştırır.
fn liste_isareti(kirp: &str) -> Option<ListeIsareti> {
    let karakterler: Vec<char> = kirp.chars().collect();
    if karakterler.len() >= 2 {
        if matches!(karakterler[0], '-' | '*' | '+') && karakterler[1] == ' ' {
            return Some(ListeIsareti {
                sirali: false,
                uzunluk: 1,
                icerik_genislik: 2,
            });
        }
        if karakterler[0].is_ascii_digit() {
            let rakam_sayisi = karakterler
                .iter()
                .take_while(|c| c.is_ascii_digit())
                .count();
            if rakam_sayisi <= 9 && rakam_sayisi + 1 < karakterler.len() {
                let ayirici = karakterler[rakam_sayisi];
                if (ayirici == '.' || ayirici == ')') && karakterler[rakam_sayisi + 1] == ' ' {
                    return Some(ListeIsareti {
                        sirali: true,
                        uzunluk: rakam_sayisi + 1,
                        icerik_genislik: rakam_sayisi + 2,
                    });
                }
            }
        }
    }
    None
}

/// Setteks alt çizgisi (`===` → 1, `---` → 2) veya `Some(0)`.
fn setext_alt_cizgi(kirp: &str) -> Option<usize> {
    if kirp.len() >= 2 && kirp.chars().all(|c| c == '=') {
        return Some(1);
    }
    if kirp.len() >= 2 && kirp.chars().all(|c| c == '-') {
        return Some(2);
    }
    None
}

/// Üç veya daha fazla `-`, `*` ya da `_` karakterinden oluşan yatay çizgi.
fn yatay_cizgidir(kirp: &str) -> bool {
    let temiz: String = kirp.chars().filter(|c| !c.is_whitespace()).collect();
    if temiz.len() < 3 {
        return false;
    }
    ['-', '*', '_']
        .iter()
        .any(|isaret| temiz.chars().all(|c| c == *isaret))
}

/// Satırın dört boşlukla girintili kod satırı olup olmadığını bildirir.
fn girintili_kod_satiridir(satir: &str) -> bool {
    satir.starts_with("    ") && !satir.trim().is_empty()
}

/// ATX başlık seviyesini döner (`#`..`######`).
fn atx_basligi(kirp: &str) -> Option<usize> {
    let sayi = kirp.chars().take_while(|c| *c == '#').count();
    if sayi == 0 || sayi > 6 {
        return None;
    }
    match kirp.chars().nth(sayi) {
        Some(' ') | None => Some(sayi),
        Some(_) => None,
    }
}

/// Kapatma çitini açar: `(delil, çit uzunluğu, dil bilgisi)`.
fn kapatma_ayikla(kirp: &str) -> (char, usize, String) {
    let karakterler: Vec<char> = kirp.chars().collect();
    let delil = karakterler.first().copied().unwrap_or('`');
    let adet = karakterler.iter().take_while(|c| **c == delil).count();
    let bilgi: String = karakterler[adet..].iter().collect();
    (delil, adet.max(3), bilgi.trim().to_string())
}

/// Bir satırın kapatma çiti başlangıcı olup olmadığını bildirir.
fn kapatma_cesididir(kirp: &str) -> bool {
    (kirp.starts_with("```") || kirp.starts_with("~~~")) && kirp.trim().len() >= 3
}

/// Satırın satır başındaki boşluk sayısını döner.
fn girinti_genisligi(satir: &str) -> usize {
    satir.chars().take_while(|c| *c == ' ').count()
}

/// Başlık satırının ardından hizalama satırı gelip gelmediğini bildirir.
fn tablo_basligi_mi(satirlar: &[&str], indeks: usize) -> bool {
    if !satirlar[indeks].contains('|') {
        return false;
    }
    match satirlar.get(indeks + 1) {
        Some(satir) => hizalama_satiri(satir).is_some(),
        None => false,
    }
}

/// GFM hizalama satırını `["", " style=\"text-align:center\"", ...]` olarak çözer.
fn hizalama_satiri(satir: &str) -> Option<Vec<Option<String>>> {
    let kirp = satir.trim();
    if !kirp.contains('-') || !kirp.contains('|') {
        return None;
    }
    let hucreler = tablo_hucreleri(kirp);
    if hucreler.is_empty() {
        return None;
    }
    let mut hizalamalar = Vec::with_capacity(hucreler.len());
    for hucre in &hucreler {
        let temiz: String = hucre.chars().filter(|c| !c.is_whitespace()).collect();
        if temiz.is_empty() {
            return None;
        }
        let sol = temiz.starts_with(':');
        let sag = temiz.ends_with(':');
        let cis = temiz.trim_matches(':');
        if cis.is_empty() || !cis.chars().all(|c| c == '-') {
            return None;
        }
        hizalamalar.push(match (sol, sag) {
            (true, true) => Some(" style=\"text-align:center\"".to_string()),
            (true, false) => Some(" style=\"text-align:left\"".to_string()),
            (false, true) => Some(" style=\"text-align:right\"".to_string()),
            (false, false) => None,
        });
    }
    Some(hizalamalar)
}

/// Tablo satırını hücre dizesine böler (`\|` kaçışını onurur).
fn tablo_hucreleri(satir: &str) -> Vec<String> {
    let kirp = satir.trim();
    let kirp = kirp.strip_prefix('|').unwrap_or(kirp);
    let kirp = kirp.strip_suffix('|').unwrap_or(kirp);
    let mut hucreler: Vec<String> = Vec::new();
    let mut mevcut = String::new();
    let karakterler: Vec<char> = kirp.chars().collect();
    let mut i = 0usize;
    while i < karakterler.len() {
        if karakterler[i] == '\\' && karakterler.get(i + 1) == Some(&'|') {
            mevcut.push('|');
            i += 2;
            continue;
        }
        if karakterler[i] == '|' {
            hucreler.push(mevcut.trim().to_string());
            mevcut = String::new();
            i += 1;
            continue;
        }
        mevcut.push(karakterler[i]);
        i += 1;
    }
    hucreler.push(mevcut.trim().to_string());
    hucreler
}

/// Metni HTML metin düğümü için kaçışlar.
fn kacis(girdi: &str) -> String {
    let mut cikti = String::with_capacity(girdi.len());
    for karakter in girdi.chars() {
        match karakter {
            '&' => cikti.push_str("&amp;"),
            '<' => cikti.push_str("&lt;"),
            '>' => cikti.push_str("&gt;"),
            '"' => cikti.push_str("&quot;"),
            _ => cikti.push(karakter),
        }
    }
    cikti
}

/// Metni HTML nitelik değeri için kaçışlar (tırnak dahil).
fn kacis_nitelik(girdi: &str) -> String {
    let mut cikti = String::with_capacity(girdi.len());
    for karakter in girdi.chars() {
        match karakter {
            '&' => cikti.push_str("&amp;"),
            '<' => cikti.push_str("&lt;"),
            '>' => cikti.push_str("&gt;"),
            '"' => cikti.push_str("&quot;"),
            '\'' => cikti.push_str("&#39;"),
            _ => cikti.push(karakter),
        }
    }
    cikti
}

/// Dil sınıfı adını güvenli hale getirir (yalnızca harf, rakam, `-`, `_`).
fn dil_sinifi(bilgi: &str) -> String {
    let ilk: String = bilgi
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '+' || *c == '-' || *c == '#')
        .collect();
    if ilk.is_empty() {
        String::new()
    } else {
        format!("language-{ilk}")
    }
}

/// Verilen konumdaki arka arkaya gelen backtick sayısını ve son konumu döner.
fn backticks_adet(karakterler: &[char], baslangic: usize) -> (usize, usize) {
    let adet = karakterler[baslangic..]
        .iter()
        .take_while(|c| **c == '`')
        .count();
    (adet, baslangic + adet)
}

/// Aynı uzunlukta kapanış backtick dizisinin başladığı konumu bulur.
fn kapat_backtick(karakterler: &[char], baslangic: usize, adet: usize) -> Option<usize> {
    let mut i = baslangic;
    while i < karakterler.len() {
        if karakterler[i] == '`' {
            let (bulunan, son) = backticks_adet(karakterler, i);
            if bulunan == adet {
                return Some(i);
            }
            i = son;
        } else {
            i += 1;
        }
    }
    None
}

/// Vurgu kapatışının başladığı konumu bulur.
///
/// Arama açılış işaretinden sonraki konumdan başlar; bu yüzden açılışın
/// kendisi eşleşme adayı olmaz.
fn kapat_vurgu(
    karakterler: &[char],
    baslangic: usize,
    isaret: char,
    genislik: usize,
) -> Option<usize> {
    let mut i = baslangic + genislik;
    while i < karakterler.len() {
        if karakterler[i] == isaret {
            let kalan_say = karakterler[i..]
                .iter()
                .take_while(|c| **c == isaret)
                .count();
            if kalan_say >= genislik {
                return Some(i);
            }
            i += kalan_say;
        } else {
            i += 1;
        }
    }
    None
}

/// `_` vurgusunun sözcük içinde başlamasını engeller.
fn onceki_kelime(karakterler: &[char], indeks: usize) -> bool {
    karakterler
        .get(indeks.wrapping_sub(1))
        .map(|c| c.is_alphanumeric())
        .unwrap_or(false)
}

/// `[metin](adres)` bağlantısını çözer; kapanış yoksa `None` döner.
fn baglanti_coz(karakterler: &[char], baslangic: usize) -> Option<(String, String, usize)> {
    let mut derinlik = 0usize;
    let mut metin_sonu = None;
    let mut i = baslangic;
    while i < karakterler.len() {
        match karakterler[i] {
            '[' => derinlik += 1,
            ']' => {
                derinlik -= 1;
                if derinlik == 0 {
                    metin_sonu = Some(i);
                    break;
                }
            }
            _ => {}
        }
        i += 1;
    }
    let metin_sonu = metin_sonu?;
    if karakterler.get(metin_sonu + 1) != Some(&'(') {
        return None;
    }
    let mut parantez = 0usize;
    let mut adres_sonu = None;
    let mut i = metin_sonu + 1;
    while i < karakterler.len() {
        match karakterler[i] {
            '(' => parantez += 1,
            ')' => {
                parantez -= 1;
                if parantez == 0 {
                    adres_sonu = Some(i);
                    break;
                }
            }
            _ => {}
        }
        i += 1;
    }
    let adres_sonu = adres_sonu?;
    let metin: String = karakterler[baslangic + 1..metin_sonu].iter().collect();
    let ham_adres: String = karakterler[metin_sonu + 2..adres_sonu].iter().collect();
    let adres = ham_adres
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_string();
    Some((metin, adres, adres_sonu + 1))
}

/// `<https://ornek.example>` otomatik bağlantısını çözer.
fn otomatik_baglanti_coz(karakterler: &[char], baslangic: usize) -> Option<(String, usize)> {
    let mut son = baslangic + 1;
    while son < karakterler.len() && karakterler[son] != '>' {
        if karakterler[son].is_whitespace() || karakterler[son] == '<' {
            return None;
        }
        son += 1;
    }
    if son >= karakterler.len() || karakterler[son] != '>' || son == baslangic + 1 {
        return None;
    }
    let adres: String = karakterler[baslangic + 1..son].iter().collect();
    if !(adres.starts_with("http://")
        || adres.starts_with("https://")
        || adres.starts_with("mailto:"))
    {
        return None;
    }
    Some((adres, son + 1))
}

#[cfg(test)]
// unwrap/expect yalnizca testlerde ve gerekçeyle kullanilabilir (WORKER_CONTRACT.md § 4.2).
// Bir testte hata durumu testin kendisidir; sessizce yutulmamalidir.
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn baslik_uretir_ve_kimlik_verir() {
        let cikti = ayikla("# Merhaba Dunya");
        assert!(cikti
            .html
            .contains("<h1 id=\"merhaba-dunya\">Merhaba Dunya</h1>"));
    }

    #[test]
    fn alti_seviyeden_fazla_baslik_paragraf_kalir() {
        let cikti = ayikla("####### Yedi");
        assert!(cikti.html.contains("<p>####### Yedi</p>"));
    }

    #[test]
    fn paragraf_satirlarini_birlestirir() {
        let cikti = ayikla("birinci satir\nikinci satir");
        assert_eq!(cikti.html, "<p>birinci satir\nikinci satir</p>\n");
    }

    #[test]
    fn kalin_ve_italik_uretir() {
        let cikti = ayikla("**kalin** ve *italik*");
        assert!(cikti.html.contains("<strong>kalin</strong>"));
        assert!(cikti.html.contains("<em>italik</em>"));
    }

    #[test]
    fn satir_ici_kod_kacislar() {
        let cikti = ayikla("`a < b` metni");
        assert!(cikti.html.contains("<code>a &lt; b</code>"));
    }

    #[test]
    fn baglanti_uretir() {
        let cikti = ayikla("[metin](https://ornek.example)");
        assert!(cikti
            .html
            .contains("<a href=\"https://ornek.example\">metin</a>"));
    }

    #[test]
    fn otomatik_baglanti_uretir() {
        let cikti = ayikla("<https://ornek.example>");
        assert!(cikti
            .html
            .contains("<a href=\"https://ornek.example\">https://ornek.example</a>"));
    }

    #[test]
    fn gorsel_uretir() {
        let cikti = ayikla("![logo](/static/logo.png)");
        assert!(cikti
            .html
            .contains("<img src=\"/static/logo.png\" alt=\"logo\" />"));
    }

    #[test]
    fn alinti_uretir() {
        let cikti = ayikla("> alinti satiri");
        assert_eq!(
            cikti.html,
            "<blockquote>\n<p>alinti satiri</p>\n</blockquote>\n"
        );
    }

    #[test]
    fn yatay_cizgi_uretir() {
        let cikti = ayikla("---");
        assert_eq!(cikti.html, "<hr />\n");
    }

    #[test]
    fn kapatmali_kod_blogu_uretir() {
        let cikti = ayikla("```rust\nlet a = 1 < 2;\n```");
        assert!(cikti
            .html
            .contains("<pre><code class=\"language-rust\">let a = 1 &lt; 2;\n</code></pre>"));
    }

    #[test]
    fn girintili_kod_blogu_uretir() {
        let cikti = ayikla("    let a = 1;\n    let b = 2;");
        assert!(cikti
            .html
            .contains("<pre><code>let a = 1;\nlet b = 2;\n</code></pre>"));
    }

    #[test]
    fn sirasiz_liste_uretir() {
        let cikti = ayikla("- bir\n- iki");
        assert_eq!(cikti.html, "<ul>\n<li>bir</li>\n<li>iki</li>\n</ul>\n");
    }

    #[test]
    fn sirali_liste_uretir() {
        let cikti = ayikla("1. bir\n2. iki");
        assert_eq!(cikti.html, "<ol>\n<li>bir</li>\n<li>iki</li>\n</ol>\n");
    }

    #[test]
    fn ic_ice_liste_uretir() {
        let cikti = ayikla("- dis\n  - ic");
        assert!(
            cikti
                .html
                .contains("<li>dis<ul>\n<li>ic</li>\n</ul>\n</li>"),
            "{}",
            cikti.html
        );
    }

    #[test]
    fn tablo_uretir() {
        let cikti = ayikla("| a | b |\n| --- | ---: |\n| 1 | 2 |");
        assert!(cikti.html.contains("<th>a</th>"));
        assert!(cikti.html.contains("<th style=\"text-align:right\">b</th>"));
        assert!(cikti.html.contains("<td>1</td>"));
        assert!(cikti.html.contains("<td style=\"text-align:right\">2</td>"));
    }

    #[test]
    fn setext_basligi_uretir() {
        let cikti = ayikla("Baslik\n=======");
        assert!(
            cikti.html.contains("<h1 id=\"baslik\">Baslik</h1>"),
            "{}",
            cikti.html
        );
    }

    #[test]
    fn bos_belge_bos_html_verir() {
        let cikti = ayikla("");
        assert_eq!(cikti.html, "");
        assert!(cikti.uyarilar.is_empty());
    }

    #[test]
    fn kapanmayan_kod_blogu_uyarir() {
        let cikti = ayikla("```rust\nlet a = 1;");
        assert_eq!(cikti.uyarilar.len(), 1);
        assert!(cikti.uyarilar[0].mesaj.contains("kapanmayan"));
    }

    #[test]
    fn kapanmayan_kod_isareti_uyarir() {
        let cikti = ayikla("`acik kalan");
        assert_eq!(cikti.uyarilar.len(), 1);
        assert!(cikti.html.contains("`"));
    }

    #[test]
    fn kapanmayan_vurgu_uyarir() {
        let cikti = ayikla("**acik kalan");
        assert!(!cikti.uyarilar.is_empty());
        assert!(cikti.html.contains("**acik kalan"));
    }

    #[test]
    fn kapanmayan_baglanti_uyarir() {
        let cikti = ayikla("[metin](");
        assert!(!cikti.uyarilar.is_empty());
        assert!(cikti.html.contains("[metin]"));
    }

    #[test]
    fn ham_html_uyarir_ve_kacislanir() {
        let cikti = ayikla("<div class=\"x\">y</div>");
        assert!(!cikti.uyarilar.is_empty());
        assert!(cikti.html.contains("&lt;div"), "{}", cikti.html);
        assert!(!cikti.html.contains("<div"));
    }

    #[test]
    fn ayni_baslik_kimligi_tekrar_sayilir() {
        let cikti = ayikla("## A\n\n## A");
        assert!(cikti.html.contains("id=\"a\""));
        assert!(cikti.html.contains("id=\"a-2\""));
    }

    #[test]
    fn turkce_baslik_kimligi_harfleri_korur() {
        let cikti = ayikla("## Nasıl çalışır?");
        assert!(
            cikti.html.contains("id=\"nasıl-çalışır\""),
            "html: {}",
            cikti.html
        );
    }

    #[test]
    fn baslik_kimligi_noktalama_ayiklanir() {
        let cikti = ayikla("## Merhaba, Dünya! (v2)");
        assert!(
            cikti.html.contains("id=\"merhaba-dünya-v2\""),
            "{}",
            cikti.html
        );
    }

    #[test]
    fn tirnak_ve_ampersand_kacislanir() {
        let cikti = ayikla("a & b \"c\"");
        assert!(cikti.html.contains("a &amp; b &quot;c&quot;"));
    }

    #[test]
    fn geri_kacis_isareti_honurlenir() {
        let cikti = ayikla("\\*duz\\*");
        assert!(cikti.html.contains("<p>*duz*</p>"));
    }

    #[test]
    fn sozcuk_ici_tire_vurgu_yapmaz() {
        let cikti = ayikla("foo_bar_baz");
        assert!(cikti.html.contains("foo_bar_baz"));
        assert!(!cikti.html.contains("<em>"));
    }

    #[test]
    fn blok_derinligi_sinirini_asmaz() {
        let girdi = "> ".repeat(200) + "derin";
        let cikti = ayikla(&girdi);
        assert!(cikti.html.len() < 200_000);
        assert!(!cikti.uyarilar.is_empty());
    }

    #[test]
    fn sesiz_helpers_dogru_html_verir() {
        assert_eq!(ayikla_sessiz("**a**"), "<p><strong>a</strong></p>\n");
    }

    #[test]
    fn uyari_gosterimi_satir_ve_mesaj_yazar() {
        let uyari = Uyari {
            satir: 3,
            mesaj: "ornek".to_string(),
        };
        assert_eq!(uyari.to_string(), "satir 3: ornek");
    }
}
