//! Uçtan uca entegrasyon testleri: gerçek klasörler, gerçek dosyalar.
//!
//! Bu dosya `tempfile` crate'i **kullanmaz** (bağımlılık politikası); geçici
//! klasör yardımcısı aşağıda tanımlıdır ve `Drop` ile temizlik yapar.
//!
//! Ağ testleri yalnızca `127.0.0.1:0` üzerinde yapılır; dışarı çıkılmaz ve port
//! tahmini yapılmaz.

use std::path::{Path, PathBuf};
use std::process::Command;

use siteturk::build::{self, UretimSecenekleri};
use siteturk::config::Yapilandirma;
use siteturk::discover;
use siteturk::serve::{self, Istek, Sunucu};

/// Test içinde geçici proje üreten, `Drop` ile temizleyen kapsayıcı.
///
/// Neden `tempfile` yok: bağımlılık politikası (`WORKER_CONTRACT.md` § 3.2)
/// `tempfile`'i hiçbir projede vermez; yardımcı kendi kodumuzla yazılır.
pub struct GeciciProje {
    yol: PathBuf,
}

impl GeciciProje {
    /// `std::env::temp_dir()` altında etiketten türetilmiş benzersiz klasör açar.
    pub fn yeni(etiket: &str) -> std::io::Result<Self> {
        let yol =
            std::env::temp_dir().join(format!("siteturk-it-{}-{}", etiket, std::process::id()));
        let _ = std::fs::remove_dir_all(&yol);
        std::fs::create_dir_all(&yol)?;
        Ok(Self { yol })
    }

    /// Klasör içine göreli dosya yazar (üst klasörleri de oluşturur).
    pub fn yaz(&self, goreli: &str, icerik: &str) -> std::io::Result<PathBuf> {
        let tam = self.yol.join(goreli);
        if let Some(ust) = tam.parent() {
            std::fs::create_dir_all(ust)?;
        }
        std::fs::write(&tam, icerik)?;
        Ok(tam)
    }

    /// Proje kökü.
    pub fn kok(&self) -> &Path {
        &self.yol
    }

    /// Kök altındaki dosyayı okur.
    pub fn oku(&self, goreli: &str) -> String {
        std::fs::read_to_string(self.yol.join(goreli)).unwrap_or_default()
    }

    /// Kök altındaki dosya var mı.
    pub fn var(&self, goreli: &str) -> bool {
        self.yol.join(goreli).exists()
    }
}

impl Drop for GeciciProje {
    fn drop(&mut self) {
        // Temizlik hatası testi düşürmemelidir; `let _ =` bilinçlidir
        // (sözleşme § 5.3, "Drop temizliği" istisnası).
        let _ = std::fs::remove_dir_all(&self.yol);
    }
}

/// Yazılabilir bir örnek site iskeleti kurar.
fn iskelet(proje: &GeciciProje) {
    proje
        .yaz(
            "siteturk.json",
            r#"{
  "kaynak": "icerik",
  "cikti": "dist",
  "sablon": "sablonlar/sayfa.html",
  "varlik": "static",
  "adres": "https://ornek.example"
}"#,
        )
        .unwrap();
    proje
        .yaz(
            "sablonlar/sayfa.html",
            "<!DOCTYPE html><html><head><title>{{ sayfa.baslik }}</title>\
             <link rel=\"stylesheet\" href=\"{{ varlik(\"sayfa.css\") }}\">\
             </head><body>\
             <h1>{{ sayfa.baslik }}</h1>{{ sayfa.govde | ham }}\
             <nav>{% for s in tumSayfalar %}<a href=\"{{ s.adres }}\">{{ s.baslik }}</a>{% endfor %}</nav>\
             <p>{{ site.sayfaSayisi }} sayfa</p>\
             </body></html>\n",
        )
        .unwrap();
    proje
        .yaz(
            "icerik/index.md",
            "---\nbaslik: Ana sayfa\ntarih: 2026-09-29\netiketler: [haber]\n---\n\nMerhaba **dunya**.\n",
        )
        .unwrap();
}

fn uret(proje: &GeciciProje) -> build::Uretim {
    let yapilandirma = Yapilandirma::yukle(proje.kok()).unwrap();
    build::uret(proje.kok(), &yapilandirma, UretimSecenekleri::default()).unwrap()
}

#[test]
fn tam_site_uretimi_basariyla_tamamlanir() {
    let proje = GeciciProje::yeni("tam").unwrap();
    iskelet(&proje);
    proje
        .yaz(
            "icerik/hakkimizda.md",
            "---\nbaslik: Hakkımızda\n---\n\nBiz kimiz?\n",
        )
        .unwrap();
    proje
        .yaz(
            "icerik/blog/neden.md",
            "---\nbaslik: Neden\n---\n\nGerekçe.\n",
        )
        .unwrap();
    proje.yaz("static/sayfa.css", "body{color:red}\n").unwrap();

    let sonuc = uret(&proje);

    assert_eq!(sonuc.rapor.toplam_dosya, 3);
    assert_eq!(sonuc.rapor.yeniden_uretilen, 3);
    assert_eq!(sonuc.rapor.varlik, 1);
    assert!(proje.var("dist/index.html"));
    assert!(proje.var("dist/hakkimizda/index.html"));
    assert!(proje.var("dist/blog/neden/index.html"));
    assert!(proje.var("dist/sitemap.xml"));
    assert!(proje.var("dist/robots.txt"));
}

#[test]
fn uretilen_html_sablon_ve_markdown_birlestirir() {
    let proje = GeciciProje::yeni("birlestir").unwrap();
    iskelet(&proje);
    proje.yaz("static/sayfa.css", "body{}").unwrap();
    uret(&proje);

    let html = proje.oku("dist/index.html");
    assert!(html.contains("<title>Ana sayfa</title>"));
    assert!(html.contains("<h1>Ana sayfa</h1>"));
    assert!(html.contains("<strong>dunya</strong>"));
    assert!(html.contains("1 sayfa"));
    // Varlık adresi imzalı hâlde yazılır.
    assert!(html.contains("/static/sayfa."), "{html}");
    assert!(html.contains(".css\""));
}

#[test]
fn kismi_yeniden_uretim_yalnizca_degiseni_isler() {
    let proje = GeciciProje::yeni("kismi").unwrap();
    iskelet(&proje);
    proje
        .yaz(
            "icerik/ikinci.md",
            "---\nbaslik: Ikinci\n---\n\nIkinci govde.\n",
        )
        .unwrap();
    proje.yaz("static/sayfa.css", "body{}").unwrap();

    let ilk = uret(&proje);
    assert_eq!(ilk.rapor.yeniden_uretilen, 2);
    assert_eq!(ilk.rapor.korunan, 0);

    let ikinci = uret(&proje);
    assert_eq!(
        ikinci.rapor.yeniden_uretilen, 0,
        "degisiklik yokken yazmamali"
    );
    assert_eq!(ikinci.rapor.korunan, 2);

    // Dosya gerçekten değişti: boyut farklı olacak biçimde yaz.
    proje
        .yaz(
            "icerik/ikinci.md",
            "---\nbaslik: Ikinci\n---\n\nIkinci govde, belirgin bicimde daha uzun bir aciklamayla.\n",
        )
        .unwrap();
    let ucuncu = uret(&proje);
    assert_eq!(
        ucuncu.rapor.yeniden_uretilen, 1,
        "yalnizca degisen sayfa yazilmali"
    );
    assert_eq!(ucuncu.rapor.korunan, 1);
    assert!(proje.oku("dist/ikinci/index.html").contains("daha uzun"));
}

#[test]
fn tam_uretim_kipi_onbellegi_erteler() {
    let proje = GeciciProje::yeni("tam-uret").unwrap();
    iskelet(&proje);
    proje.yaz("static/sayfa.css", "body{}").unwrap();
    uret(&proje);
    let yapilandirma = Yapilandirma::yukle(proje.kok()).unwrap();
    let sonuc = build::uret(
        proje.kok(),
        &yapilandirma,
        UretimSecenekleri { tam_uretim: true },
    )
    .unwrap();
    assert_eq!(sonuc.rapor.yeniden_uretilen, 1);
    assert_eq!(sonuc.rapor.korunan, 0);
}

#[test]
fn varlik_kopyalanir_kucultulur_ve_baglanti_yeniden_yazilir() {
    let proje = GeciciProje::yeni("varlik").unwrap();
    iskelet(&proje);
    proje
        .yaz("static/sayfa.css", "body{color:red}\n\n\n   \n")
        .unwrap();
    let sonuc = uret(&proje);
    assert_eq!(sonuc.rapor.varlik, 1);

    let imzali_ad = &sonuc.varliklar.dosyalar[0].cikti_yol;
    let govde = proje.oku(&format!("dist/{imzali_ad}"));
    // Boş satırlar atıldı.
    assert_eq!(govde.lines().count(), 1, "govde: {govde:?}");
    // Aynı içerik aynı imzayı üretir.
    let ikinci = uret(&proje);
    assert_eq!(ikinci.varliklar.dosyalar[0].cikti_yol, *imzali_ad);
}

#[test]
fn varlik_css_icindeki_baglanti_yeniden_yazilir() {
    let proje = GeciciProje::yeni("varlik-bag").unwrap();
    iskelet(&proje);
    proje.yaz("static/img/arka.png", "PNGDATA").unwrap();
    // Kaynak CSS, diğer bir varlığın **imzasız** adresine bağlı olsun.
    proje
        .yaz(
            "static/sayfa.css",
            "body{background:url(/static/img/arka.png)}",
        )
        .unwrap();

    let sonuc = uret(&proje);
    let css_yolu = sonuc
        .varliklar
        .dosyalar
        .iter()
        .find(|k| k.kaynak_yol == "sayfa.css")
        .map(|k| k.cikti_yol.clone())
        .expect("css kaydi bulunamadi");
    let yeni_adres = sonuc.varliklar.adres("img/arka.png");
    assert!(
        yeni_adres.contains("arka."),
        "adres imzali olmali: {yeni_adres}"
    );

    let yeniden = proje.oku(&format!("dist/{css_yolu}"));
    assert!(yeniden.contains(&yeni_adres), "css: {yeniden}");
    assert!(
        !yeniden.contains("url(/static/img/arka.png)"),
        "eski adres kalmaliydi degil: {yeniden}"
    );
}

#[test]
fn taslak_sayfalar_uretilmez_ama_robots_a_eklenir() {
    let proje = GeciciProje::yeni("taslak").unwrap();
    iskelet(&proje);
    proje.yaz("static/sayfa.css", "body{}").unwrap();
    proje
        .yaz(
            "icerik/blog/taslak.md",
            "---\nbaslik: Taslak\ntaslak: true\n---\n\nGizli icerik.\n",
        )
        .unwrap();

    let sonuc = uret(&proje);
    assert_eq!(sonuc.rapor.taslak, 1);
    assert!(!proje.var("dist/blog/taslak/index.html"));
    assert!(proje
        .oku("dist/robots.txt")
        .contains("Disallow: /blog/taslak/"));
    let harita = proje.oku("dist/sitemap.xml");
    assert!(!harita.contains("/blog/taslak/"), "harita: {harita}");
}

#[test]
fn sitemap_her_sayfayi_icerir() {
    let proje = GeciciProje::yeni("sitemap").unwrap();
    iskelet(&proje);
    proje.yaz("static/sayfa.css", "body{}").unwrap();
    proje
        .yaz("icerik/blog/neden.md", "---\nbaslik: N\n---\n\nG.")
        .unwrap();
    proje
        .yaz("icerik/hakkimizda.md", "---\nbaslik: H\n---\n\nK.")
        .unwrap();
    uret(&proje);

    let harita = proje.oku("dist/sitemap.xml");
    for adres in [
        "https://ornek.example/",
        "https://ornek.example/hakkimizda/",
        "https://ornek.example/blog/neden/",
    ] {
        assert!(harita.contains(adres), "harita: {harita}");
    }
    assert!(harita.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"));
    assert!(harita.contains("</urlset>"));
}

#[test]
fn ayni_girdiden_ayni_cikti_uretilir() {
    let proje = GeciciProje::yeni("tekrarlanabilir").unwrap();
    iskelet(&proje);
    proje.yaz("static/sayfa.css", "body{}").unwrap();
    proje
        .yaz("icerik/z.md", "---\nbaslik: Z\n---\n\nSon.")
        .unwrap();
    proje
        .yaz("icerik/a.md", "---\nbaslik: A\n---\n\nIlk.")
        .unwrap();
    let yapilandirma = Yapilandirma::yukle(proje.kok()).unwrap();

    build::uret(
        proje.kok(),
        &yapilandirma,
        UretimSecenekleri { tam_uretim: true },
    )
    .unwrap();
    let ilk = proje.oku("dist/index.html");
    build::uret(
        proje.kok(),
        &yapilandirma,
        UretimSecenekleri { tam_uretim: true },
    )
    .unwrap();
    let ikinci = proje.oku("dist/index.html");

    assert_eq!(ilk, ikinci, "ayni girdiden ayni cikti uretilmeli");
    assert!(ilk.contains("/a/") && ikinci.contains("/a/"));
}

#[test]
fn bozuk_on_bilgi_uretimi_durdurur_ve_dosya_satir_soyler() {
    let proje = GeciciProje::yeni("bozuk-fm").unwrap();
    iskelet(&proje);
    proje
        .yaz(
            "icerik/kirik.md",
            "---\nbaslik: T\nyazim: hata\n---\n\nG.\n",
        )
        .unwrap();
    let yapilandirma = Yapilandirma::yukle(proje.kok()).unwrap();
    let hata = build::uret(proje.kok(), &yapilandirma, UretimSecenekleri::default()).unwrap_err();
    let metin = hata.to_string();
    assert!(metin.contains("kirik.md"), "{metin}");
    assert!(metin.contains("tanimsiz alan"), "{metin}");
    assert!(metin.contains(":3"), "satir numarasi bekleniyordu: {metin}");
}

#[test]
fn bozuk_yapilandirma_uretimden_oncesinde_durur() {
    let proje = GeciciProje::yeni("bozuk-yap").unwrap();
    iskelet(&proje);
    proje.yaz("siteturk.json", "{ bozuk json").unwrap();
    let hata = Yapilandirma::yukle(proje.kok()).unwrap_err();
    assert!(hata.to_string().contains("yapilandirma hatasi"), "{hata}");
    assert!(!proje.var("dist/index.html"), "uretilmemis olmali");
}

#[test]
fn taslak_disi_alanlari_yapilandirmada_gecersizdir() {
    let proje = GeciciProje::yeni("gecersiz-alan").unwrap();
    iskelet(&proje);
    proje
        .yaz("siteturk.json", r#"{"bilinmeyen": true}"#)
        .unwrap();
    let hata = Yapilandirma::yukle(proje.kok()).unwrap_err();
    assert!(hata.to_string().contains("unknown field"), "{hata}");
}

#[test]
fn turkce_dosya_ve_klasor_adlari_uretilir() {
    let proje = GeciciProje::yeni("turkce").unwrap();
    iskelet(&proje);
    proje.yaz("static/sayfa.css", "body{}").unwrap();
    proje
        .yaz(
            "icerik/hakkımızda/index.md",
            "---\nbaslik: Hakkımızda\n---\n\nBiz.\n",
        )
        .unwrap();
    uret(&proje);
    assert!(proje.var("dist/hakkımızda/index.html"));
    let html = proje.oku("dist/hakkımızda/index.html");
    assert!(html.contains("Hakkımızda"));
    assert!(proje.oku("dist/sitemap.xml").contains("/hakkımızda/"));
}

#[test]
fn slug_on_bilgisi_adresi_geresalar() {
    let proje = GeciciProje::yeni("slug").unwrap();
    iskelet(&proje);
    proje.yaz("static/sayfa.css", "body{}").unwrap();
    proje
        .yaz(
            "icerik/eskiler/yazi.md",
            "---\nbaslik: Y\nslug: yeni-adres\n---\n\nG.\n",
        )
        .unwrap();
    uret(&proje);
    assert!(proje.var("dist/yeni-adres/index.html"));
    assert!(proje.oku("dist/sitemap.xml").contains("/yeni-adres/"));
}

#[test]
fn olu_dosyalar_sonraki_uretimde_temizlenir() {
    let proje = GeciciProje::yeni("olu").unwrap();
    iskelet(&proje);
    proje.yaz("static/sayfa.css", "body{}").unwrap();
    proje
        .yaz("icerik/gecici.md", "---\nbaslik: G\n---\n\nG.")
        .unwrap();
    uret(&proje);
    assert!(proje.var("dist/gecici/index.html"));

    std::fs::remove_file(proje.kok().join("icerik/gecici.md")).unwrap();
    let sonuc = uret(&proje);
    assert_eq!(sonuc.rapor.silinen, 1);
    assert!(!proje.var("dist/gecici/index.html"));
}

#[test]
fn atomik_yazma_gecici_dosya_birakmaz() {
    let proje = GeciciProje::yeni("atomik").unwrap();
    iskelet(&proje);
    proje.yaz("static/sayfa.css", "body{}").unwrap();
    uret(&proje);

    let gecici: Vec<PathBuf> = std::fs::read_dir(proje.kok().join("dist"))
        .unwrap()
        .filter_map(|g| g.ok())
        .map(|g| g.path())
        .filter(|p| {
            p.file_name()
                .map(|a| a.to_string_lossy().starts_with(build::GECICI_ONEK))
                .unwrap_or(false)
        })
        .collect();
    assert!(gecici.is_empty(), "gecici dosya kaldi: {gecici:?}");
}

#[test]
fn markdown_uyarilari_konsola_yazilir_ama_uretim_durur_maz() {
    let proje = GeciciProje::yeni("uyari").unwrap();
    iskelet(&proje);
    proje.yaz("static/sayfa.css", "body{}").unwrap();
    proje
        .yaz(
            "icerik/uc.md",
            "---\nbaslik: U\n---\n\n```rust\nacik kalan\n",
        )
        .unwrap();
    let sonuc = uret(&proje);
    assert!(sonuc.rapor.uyari >= 1);
    assert!(sonuc.uyarilar.iter().any(|u| u.contains("uc.md")));
    assert!(
        proje.var("dist/uc/index.html"),
        "uyari uretimi durdurmamali"
    );
}

#[test]
fn sunucu_yoklama_ucu_sayfa_sunar_ve_betik_enjekte_edilir() {
    let proje = GeciciProje::yeni("sunucu").unwrap();
    iskelet(&proje);
    proje.yaz("static/sayfa.css", "body{}").unwrap();
    uret(&proje);

    let dist = proje.kok().join("dist");
    let mut sunucu = Sunucu::baslat(&dist, 0).unwrap();
    let istek = Istek {
        yontem: "GET".to_string(),
        yol: "/".to_string(),
        sorgu: String::new(),
        surum: "HTTP/1.1".to_string(),
    };
    let yanit = sunucu.yanit_uret(&istek);
    assert_eq!(yanit.kod, 200);
    let govde = String::from_utf8_lossy(&yanit.govde);
    assert!(govde.contains("Ana sayfa"));
    assert!(
        govde.contains(serve::DURUM_YOLU),
        "canli yenileme betigi enjekte edilmedi"
    );
}

#[test]
fn sunucu_kok_disina_cikmayi_reddeder() {
    let proje = GeciciProje::yeni("kacis").unwrap();
    iskelet(&proje);
    proje.yaz("static/sayfa.css", "body{}").unwrap();
    uret(&proje);

    // Sunucu kökünün DIŞINDA gizli bir dosya.
    let dis = proje.kok().join("../siteturk-gizli.txt");
    std::fs::write(&dis, "GIZLI ICERIK").unwrap();

    let dist = proje.kok().join("dist");
    let mut sunucu = Sunucu::baslat(&dist, 0).unwrap();

    for kacis_yolu in [
        "/../siteturk-gizli.txt",
        "/../../siteturk-gizli.txt",
        "/blog/../../siteturk-gizli.txt",
        "/..%2F..%2Fsiteturk-gizli.txt",
    ] {
        let istek = Istek {
            yontem: "GET".to_string(),
            yol: kacis_yolu.to_string(),
            sorgu: String::new(),
            surum: "HTTP/1.1".to_string(),
        };
        let yanit = sunucu.yanit_uret(&istek);
        let govde = String::from_utf8_lossy(&yanit.govde);
        assert!(
            yanit.kod == 400 || yanit.kod == 403 || yanit.kod == 404,
            "{} icin kod {}",
            kacis_yolu,
            yanit.kod
        );
        assert!(
            !govde.contains("GIZLI ICERIK"),
            "{} ile kurtarma basarili!",
            kacis_yolu
        );
    }
    let _ = std::fs::remove_file(&dis);
}

#[test]
fn sunucu_yalniz_yerel_adrese_baglanir() {
    let proje = GeciciProje::yeni("yerel").unwrap();
    iskelet(&proje);
    proje.yaz("static/sayfa.css", "body{}").unwrap();
    uret(&proje);
    let dist = proje.kok().join("dist");
    let sunucu = Sunucu::baslat(&dist, 0).unwrap();
    let adres = sunucu.yerel_adres();
    assert!(adres.starts_with("127.0.0.1:"), "adres: {adres}");
    assert!(!adres.starts_with("0.0.0.0"), "adres: {adres}");
    assert!(!adres.contains(':').eq(&false));
}

#[test]
fn http_istek_ayiklama_ve_dosya_sunumu_ucltan_uca_calisir() {
    use std::io::{Read, Write};
    use std::net::TcpStream;

    let proje = GeciciProje::yeni("ucltan-uca").unwrap();
    iskelet(&proje);
    proje.yaz("static/sayfa.css", "body{}").unwrap();
    uret(&proje);

    let dist = proje.kok().join("dist");
    let mut sunucu = Sunucu::baslat(&dist, 0).unwrap();
    let adres = sunucu.yerel_adres();

    // Sunucuyu ayrı bir iş parçacığında çalıştır, istiği ana iş parçacığından gönder.
    let istek = Istek {
        yontem: "GET".to_string(),
        yol: "/index.html".to_string(),
        sorgu: String::new(),
        surum: "HTTP/1.1".to_string(),
    };
    let _ = istek;
    let adres_kopya = adres.clone();
    let isleyici = std::thread::spawn(move || {
        // Sunucu bağlantı yokken bloklamaz, `false` döner; canlı yenileme
        // döngüsünün yaptığı gibi tek isteği işleyene kadar yoklanır.
        for _ in 0..1000 {
            if sunucu.tek_istek_isle().unwrap_or(false) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    });
    let mut akis = TcpStream::connect(&adres_kopya).expect("yerel baglanti kurulamadi");
    akis.write_all(b"GET /index.html HTTP/1.1\r\nHost: localhost\r\n\r\n")
        .unwrap();
    let mut cevap = String::new();
    akis.read_to_string(&mut cevap).unwrap();
    isleyici.join().unwrap();

    assert!(cevap.starts_with("HTTP/1.1 200 OK"), "cevap: {cevap}");
    assert!(cevap.contains("Content-Type: text/html"), "cevap: {cevap}");
    assert!(cevap.contains("Ana sayfa"), "cevap: {cevap}");
}

#[test]
fn kesif_kurali_turkce_ve_ingilizce_karisimini_ayirt_eder() {
    let proje = GeciciProje::yeni("kesif").unwrap();
    iskelet(&proje);
    proje
        .yaz("icerik/hakkimizda.md", "---\nbaslik: A\n---\nG.")
        .unwrap();
    proje
        .yaz("icerik/hakkımızda.md", "---\nbaslik: B\n---\nG.")
        .unwrap();
    let bulunan = discover::markdown_dosyalarini_bul(&proje.kok().join("icerik")).unwrap();
    let adlar: Vec<&str> = bulunan.iter().map(|d| d.goreli.as_str()).collect();
    assert!(adlar.contains(&"hakkimizda.md"));
    assert!(adlar.contains(&"hakkımızda.md"));
    assert_eq!(bulunan.len(), 3, "adlar: {adlar:?}");
}

#[test]
fn ikili_yemek_uretilir_ve_cikis_kodu_basarilidir() {
    let proje = GeciciProje::yeni("ikili").unwrap();
    iskelet(&proje);
    proje.yaz("static/sayfa.css", "body{}").unwrap();

    let ikili = std::env::current_exe()
        .ok()
        .and_then(|y| y.parent().map(|p| p.join("siteturk").to_path_buf()))
        .or_else(|| {
            let aday = std::env::current_exe().ok()?;
            Some(aday)
        });
    if ikili.is_none() {
        return; // Test ikilisi (lib) altinda calisiyorsa atlanir.
    }
    let ikili = ikili.unwrap();
    if !ikili.exists() {
        return;
    }
    let cikti = Command::new(&ikili)
        .arg("--proje")
        .arg(proje.kok())
        .arg("build")
        .output()
        .expect("ikili calistirilamadi");
    assert!(
        cikti.status.success(),
        "cikis: {}",
        String::from_utf8_lossy(&cikti.stderr)
    );
    let stdout = String::from_utf8_lossy(&cikti.stdout);
    assert!(stdout.contains("uretildi:"), "stdout: {stdout}");
    assert!(proje.var("dist/index.html"));
}
