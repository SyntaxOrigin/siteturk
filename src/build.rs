//! Yeniden üretim orkestrasyonu ve kısmi (incremental) yeniden üretim önbelleği.
//!
//! Önbellek, her içerik dosyasının `mtime` + boyut imzasını bir JSON dosyasında
//! tutar. Değişmeyen dosya **yeniden ayrıştırılmaz ve yeniden yazılmaz**; bu,
//! raporun "kısmi iyileştirme" gerekçesinin CLI karşılığıdır.
//!
//! `mtime` çözünürlüğü dosya sistemine göre değişebileceğinden `--tam-uretim`
//! kipi sağlanır; önbellek güvenilmez olduğunda tam üretim zorunlu tutulabilir.
//!
//! Yazma **atomiktir**: önce geçici dosyaya, sonra hedefe yazılır. Böylece
//! üretim yarıda kesilse bile çıktı klasöründe yarım dosya kalmaz.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::assets::{self, VarlikRaporu};
use crate::bom_kaldir;
use crate::config::Yapilandirma;
use crate::discover::{self, IcerikDosyasi, Imza};
use crate::error::Hata;
use crate::frontmatter::OnBilgi;
use crate::markdown;
use crate::sitemap::{self, HaritaGirdisi};
use crate::template::{self, Deger};

/// Geçici dosya adı öneki; dağıtım öncesi denetim bunları tanır.
pub const GECICI_ONEK: &str = ".siteturk-gecici-";

/// Bir dosyanın önceki üretimdeki imzası.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct ImzaKaydi {
    /// `mtime` nanosaniye.
    mtime: u128,
    /// Bayt cinsinden boyut.
    boyut: u64,
}

impl From<Imza> for ImzaKaydi {
    fn from(deger: Imza) -> Self {
        ImzaKaydi {
            mtime: deger.mtime,
            boyut: deger.boyut,
        }
    }
}

impl From<&ImzaKaydi> for Imza {
    fn from(deger: &ImzaKaydi) -> Self {
        Imza {
            mtime: deger.mtime,
            boyut: deger.boyut,
        }
    }
}

/// Yeniden üretim önbelleğinin diske yazılan hâli.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Onbellek {
    /// Göreli içerik yolu → imza.
    #[serde(default)]
    pub dosyalar: BTreeMap<String, ImzaKaydi>,
    /// Son üretimde yazılan çıktı dosyaları; ölü dosya temizliğinde kullanılır.
    #[serde(default)]
    pub ciktilar: Vec<String>,
}

impl Onbellek {
    /// Önbelleğin dosya yolu.
    pub fn yol(kok: &Path) -> PathBuf {
        kok.join(".siteturk-onbellek.json")
    }

    /// Önbelleği okur; dosya yoksa boş önbellek döner.
    ///
    /// Bozuk önbellek **hata değildir**: bu durumda tam üretim yapılır. Önbellek
    /// bir hızlandırıcıdır, kaynak değil.
    pub fn oku(kok: &Path) -> Self {
        let yol = Onbellek::yol(kok);
        match std::fs::read_to_string(&yol) {
            Ok(metin) => serde_json::from_str(&metin).unwrap_or_default(),
            Err(_) => Onbellek::default(),
        }
    }

    /// Önbelleği diske yazar.
    pub fn yaz(&self, kok: &Path) -> Result<(), Hata> {
        let yol = Onbellek::yol(kok);
        let metin = serde_json::to_string(self).map_err(|hata| Hata::Yapilandirma {
            yol: yol.display().to_string(),
            ayrinti: hata.to_string(),
        })?;
        yaz_atomik(&yol, metin.as_bytes())
    }
}

/// Bir üretimin sayısal özeti.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UretimRaporu {
    /// Keşfedilen toplam Markdown dosyası.
    pub toplam_dosya: usize,
    /// Yeniden derlenen sayfa sayısı.
    pub yeniden_uretilen: usize,
    /// Önbellek sayesinde atlanan sayfa sayısı.
    pub korunan: usize,
    /// Yayımlanmayan taslak sayfası.
    pub taslak: usize,
    /// Kopyalanan varlık sayısı.
    pub varlik: usize,
    /// Bağlantısı yeniden yazılan varlık sayısı.
    pub yeniden_yazilan_varlik: usize,
    /// Silinen ölü çıktı dosyası sayısı.
    pub silinen: usize,
    /// Markdown ayrıştırıcısının ürettiği uyarı satırı sayısı.
    pub uyari: usize,
}

/// Bir üretimin tam çıktısı.
#[derive(Debug, Clone)]
pub struct Uretim {
    /// Sayısal özet.
    pub rapor: UretimRaporu,
    /// Varlık ad → adres eşlemesi.
    pub varliklar: VarlikRaporu,
    /// Üretilen sayfaların adresleri (sitemap ve sunucu için).
    pub adresler: Vec<String>,
    /// Markdown uyarılarının tam metni (kullanıcıya gösterilir).
    pub uyarilar: Vec<String>,
    /// Yalnızca sayfa yazımının geçtiği süre (milisecondse).
    ///
    /// Testlerin kararında kullanılmaz; yalnızca bilgilendirme amaçlıdır.
    pub sure_ms: u128,
}

/// Bir sayfanın tamamı üretilmek üzere hazırlanmış hâli.
#[derive(Debug, Clone)]
struct Sayfa {
    goreli: String,
    imza: Imza,
    adres: String,
    cikti: String,
    on_bilgi: OnBilgi,
    govde_html: String,
    guncellenme: u64,
}

/// Üretim seçenekleri.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UretimSecenekleri {
    /// Önbelleği yok say, her şeyi yeniden yaz.
    pub tam_uretim: bool,
}

/// Siteyi üretir.
///
/// Girdi: proje kökü, yapılandırma ve seçenekler. Çıktı: yalnızca
/// `yapilandirma.cikti` klasörüne yazılır; kaynak klasörüne hiçbir şey yazılmaz.
pub fn uret(
    kok: &Path,
    yapilandirma: &Yapilandirma,
    secenekler: UretimSecenekleri,
) -> Result<Uretim, Hata> {
    let kaynak = kok.join(&yapilandirma.kaynak);
    let cikti_kok = kok.join(&yapilandirma.cikti);
    let sablon_yol = kok.join(&yapilandirma.sablon);

    let mut rapor = UretimRaporu::default();
    if !kaynak.is_dir() {
        return Err(Hata::Kullanim {
            ayrinti: format!(
                "icerik klasoru bulunamadi: {} (yapilandirmada `kaynak`)",
                kaynak.display()
            ),
        });
    }

    let sablon_kaynak = std::fs::read_to_string(&sablon_yol).map_err(|hata| {
        if hata.kind() == std::io::ErrorKind::NotFound {
            Hata::Kullanim {
                ayrinti: format!(
                    "sablon dosyasi bulunamadi: {} (yapilandirmada `sablon`)",
                    sablon_yol.display()
                ),
            }
        } else {
            Hata::disk(&sablon_yol, &hata)
        }
    })?;
    let sablon = template::Sablon::ayikla(
        &yapilandirma.sablon.display().to_string(),
        bom_kaldir(&sablon_kaynak),
    )?;

    // Varlıklar sayfalardan önce üretilir: şablonlar `varlik(...)` ile adres ister.
    let varliklar = assets::isle(
        &kok.join(&yapilandirma.varlik),
        &cikti_kok,
        yapilandirma.imzali_varlik,
        yapilandirma.kucult,
    )?;
    rapor.varlik = varliklar.dosyalar.len();

    let dosyalar = discover::markdown_dosyalarini_bul(&kaynak)?;
    rapor.toplam_dosya = dosyalar.len();
    let eski = Onbellek::oku(kok);
    let mut yeni = Onbellek::default();
    let mut uyarilar: Vec<String> = Vec::new();
    let mut sayfalar: Vec<Sayfa> = Vec::new();
    let mut taslak_adresleri: Vec<String> = Vec::new();

    for dosya in &dosyalar {
        let hazir = sayfa_hazirla(dosya, yapilandirma, kok)?;
        for uyari in &hazir.uyarilar {
            uyarilar.push(format!("{}:{uyari}", dosya.goreli));
        }
        rapor.uyari += hazir.uyarilar.len();
        let sayfa = hazir.sayfa;

        if sayfa.on_bilgi.taslak && !yapilandirma.taslaklari_yayinla {
            rapor.taslak += 1;
            taslak_adresleri.push(sayfa.adres.clone());
            yeni.dosyalar
                .insert(dosya.goreli.clone(), dosya.imza.into());
            continue;
        }
        sayfalar.push(sayfa);
        yeni.dosyalar
            .insert(dosya.goreli.clone(), dosya.imza.into());
    }

    // Site haritası adres çakışmasını üretimden önce yakalar.
    let harita_girdileri: Vec<HaritaGirdisi> = sayfalar
        .iter()
        .map(|s| HaritaGirdisi {
            adres: s.adres.clone(),
            guncellenme: s.guncellenme,
        })
        .collect();
    let sitemap_metni = if yapilandirma.sitemap {
        Some(sitemap::sitemap_uret(
            &yapilandirma.adres,
            &harita_girdileri,
        )?)
    } else {
        None
    };

    // Menü sırası `siralama` alanına göre **küçükten büyüğe**, ardından çıktı
    // yoluna göre kararlıdır: `siralama: 1` her zaman ilk sayfada görünür.
    let mut sirali: Vec<Sayfa> = sayfalar.clone();
    sirali.sort_by(|a, b| {
        a.on_bilgi
            .siralama
            .unwrap_or(i64::MAX)
            .cmp(&b.on_bilgi.siralama.unwrap_or(i64::MAX))
            .then_with(|| a.cikti.cmp(&b.cikti))
    });
    let sirali: Vec<&Sayfa> = sirali.iter().collect();

    std::fs::create_dir_all(&cikti_kok).map_err(|hata| Hata::disk(&cikti_kok, &hata))?;

    let baslam = std::time::Instant::now();
    for sayfa in &sirali {
        let hedef = cikti_kok.join(&sayfa.cikti);
        // Kısmi üretim: içerik imzası değişmemiş **ve** çıktı hâlâ diskte ise
        // sayfa yeniden ayrıştırılmaz, yeniden yazılmaz.
        let imza_ayni = eski
            .dosyalar
            .get(&sayfa.goreli)
            .is_some_and(|kayit| *kayit == ImzaKaydi::from(sayfa.imza));
        let cikti_daha_once_yazildi = eski.ciktilar.contains(&sayfa.cikti);
        let atlanabilir =
            !secenekler.tam_uretim && imza_ayni && cikti_daha_once_yazildi && hedef.exists();
        if atlanabilir {
            rapor.korunan += 1;
            yeni.ciktilar.push(sayfa.cikti.clone());
            continue;
        }
        let baglam = baglam_olustur(yapilandirma, sayfa, &varliklar, &sirali, &taslak_adresleri);
        let html = sablon.calistir(&baglam)?;
        yaz_atomik(&hedef, html.as_bytes())?;
        yeni.ciktilar.push(sayfa.cikti.clone());
        rapor.yeniden_uretilen += 1;
    }
    let sure_ms = baslam.elapsed().as_millis();

    if let Some(metin) = sitemap_metni {
        yaz_atomik(&cikti_kok.join("sitemap.xml"), metin.as_bytes())?;
        yeni.ciktilar.push("sitemap.xml".to_string());
    }
    if yapilandirma.robots {
        let metin = sitemap::robots_uret(&yapilandirma.adres, &taslak_adresleri);
        yaz_atomik(&cikti_kok.join("robots.txt"), metin.as_bytes())?;
        yeni.ciktilar.push("robots.txt".to_string());
    }

    rapor.yeniden_yazilan_varlik = assets::baglantilari_yeniden_yaz(&varliklar, &cikti_kok)?;
    for kayit in &varliklar.dosyalar {
        let yol = kayit.cikti_yol.clone();
        if yeni.ciktilar.iter().all(|c| *c != yol) {
            yeni.ciktilar.push(yol);
        }
    }

    rapor.silinen = olu_dosyalari_temizle(&eski.ciktilar, &yeni.ciktilar, &cikti_kok)?;
    yeni.yaz(kok)?;

    Ok(Uretim {
        rapor,
        adresler: sirali.iter().map(|s| s.adres.clone()).collect(),
        varliklar,
        uyarilar,
        sure_ms,
    })
}

/// Bir içerik dosyasını tam sayfa modeline dönüştürür.
fn sayfa_hazirla(
    dosya: &IcerikDosyasi,
    yapilandirma: &Yapilandirma,
    _kok: &Path,
) -> Result<HazirSayfa, Hata> {
    let kaynak =
        std::fs::read_to_string(&dosya.kaynak).map_err(|hata| Hata::disk(&dosya.kaynak, &hata))?;
    let (on_bilgi, govde) = OnBilgi::ayikla(&dosya.goreli, bom_kaldir(&kaynak))?;
    let ayiklanmis = markdown::ayikla(&govde);
    let uyarilar: Vec<String> = ayiklanmis
        .uyarilar
        .iter()
        .map(|u| format!("{} {}", u.satir, u.mesaj))
        .collect();
    let (cikti, adres) = on_bilgi
        .slug
        .as_ref()
        .map(|slug| {
            let temiz = slug.trim_matches('/');
            (format!("{temiz}/index.html"), format!("/{temiz}/"))
        })
        .unwrap_or_else(|| {
            let cikti = if dosya.goreli == "index.md" {
                "index.html".to_string()
            } else {
                let govde = dosya.goreli.strip_suffix(".md").unwrap_or(&dosya.goreli);
                match govde.rfind('/') {
                    Some(konum) if &govde[konum + 1..] == "index" => {
                        format!("{}/index.html", &govde[..konum])
                    }
                    _ => format!("{govde}/index.html"),
                }
            };
            (cikti, dosya.adres.clone())
        });
    let _ = yapilandirma;
    Ok(HazirSayfa {
        sayfa: Sayfa {
            goreli: dosya.goreli.clone(),
            imza: dosya.imza,
            adres,
            cikti,
            on_bilgi,
            govde_html: ayiklanmis.html,
            guncellenme: (dosya.imza.mtime / 1_000_000_000) as u64,
        },
        uyarilar,
    })
}

/// Diskten okunan, henüz şablona bağlanmamış sayfa.
struct HazirSayfa {
    sayfa: Sayfa,
    uyarilar: Vec<String>,
}

/// Şablona verilecek tam bağlamı kurar.
fn baglam_olustur(
    yapilandirma: &Yapilandirma,
    sayfa: &Sayfa,
    varliklar: &VarlikRaporu,
    tum_sayfalar: &[&Sayfa],
    taslaklar: &[String],
) -> Deger {
    let mut sayfa_haritasi: BTreeMap<String, Deger> = BTreeMap::new();
    sayfa_haritasi.insert(
        "baslik".to_string(),
        Deger::metin(sayfa.on_bilgi.baslik.clone().unwrap_or_default()),
    );
    sayfa_haritasi.insert(
        "ozet".to_string(),
        Deger::metin(sayfa.on_bilgi.ozet.clone().unwrap_or_default()),
    );
    sayfa_haritasi.insert(
        "yazar".to_string(),
        Deger::metin(sayfa.on_bilgi.yazar.clone().unwrap_or_default()),
    );
    sayfa_haritasi.insert(
        "tarih".to_string(),
        Deger::metin(sayfa.on_bilgi.tarih.clone().unwrap_or_default()),
    );
    sayfa_haritasi.insert("adres".to_string(), Deger::metin(sayfa.adres.clone()));
    sayfa_haritasi.insert(
        "taslak".to_string(),
        Deger::Sayi(i64::from(sayfa.on_bilgi.taslak)),
    );
    sayfa_haritasi.insert(
        "etiketler".to_string(),
        Deger::liste(
            sayfa
                .on_bilgi
                .etiketler
                .iter()
                .map(|e| Deger::metin(e.clone()))
                .collect::<Vec<_>>(),
        ),
    );
    sayfa_haritasi.insert("govde".to_string(), Deger::metin(sayfa.govde_html.clone()));

    let liste: Vec<Deger> = tum_sayfalar
        .iter()
        .filter(|s| !s.on_bilgi.taslak)
        .map(|s| {
            let mut alt: BTreeMap<String, Deger> = BTreeMap::new();
            alt.insert(
                "baslik".to_string(),
                Deger::metin(s.on_bilgi.baslik.clone().unwrap_or_default()),
            );
            alt.insert("adres".to_string(), Deger::metin(s.adres.clone()));
            alt.insert(
                "ozet".to_string(),
                Deger::metin(s.on_bilgi.ozet.clone().unwrap_or_default()),
            );
            alt.insert(
                "tarih".to_string(),
                Deger::metin(s.on_bilgi.tarih.clone().unwrap_or_default()),
            );
            Deger::Harita(alt)
        })
        .collect();

    let mut kok: BTreeMap<String, Deger> = BTreeMap::new();
    let mut site = yapilandirma.site_baglami();
    let yayimlanan = tum_sayfalar.iter().filter(|s| !s.on_bilgi.taslak).count();
    if let Deger::Harita(harita) = &mut site {
        harita.insert("sayfaSayisi".to_string(), Deger::Sayi(yayimlanan as i64));
        harita.insert(
            "varliklar".to_string(),
            Deger::harita(
                varliklar
                    .harita
                    .iter()
                    .map(|(k, v)| (k.clone(), Deger::metin(v.clone()))),
            ),
        );
        harita.insert(
            "taslaklar".to_string(),
            Deger::liste(taslaklar.iter().map(|a| Deger::metin(a.clone()))),
        );
    }
    kok.insert("site".to_string(), site);
    kok.insert("sayfa".to_string(), Deger::Harita(sayfa_haritasi));
    kok.insert("tumSayfalar".to_string(), Deger::liste(liste));
    Deger::Harita(kok)
}

/// Artık var olmayan üretim dosyalarını siler.
fn olu_dosyalari_temizle(
    eski: &[String],
    yeni: &[String],
    cikti_kok: &Path,
) -> Result<usize, Hata> {
    let mut silinen = 0usize;
    for yol in eski {
        if yeni.iter().any(|n| n == yol) {
            continue;
        }
        if !discover::guvenli_goreli(yol) {
            // Şüpheli yol silinmez; üretim yol güvenliği kuralını ihlal eder.
            continue;
        }
        let tam = cikti_kok.join(yol);
        if tam.is_file() && std::fs::remove_file(&tam).is_ok() {
            silinen += 1;
        }
    }
    Ok(silinen)
}

/// Dosyayı önce geçici ada, sonra hedefe yazar (atomik yazma).
pub fn yaz_atomik(hedef: &Path, baytlar: &[u8]) -> Result<(), Hata> {
    if let Some(ust) = hedef.parent() {
        std::fs::create_dir_all(ust).map_err(|hata| Hata::disk(ust, &hata))?;
    }
    let ad = hedef
        .file_name()
        .map(|a| a.to_string_lossy().to_string())
        .unwrap_or_else(|| "cikti".to_string());
    let gecici = hedef.with_file_name(format!("{GECICI_ONEK}{ad}"));
    std::fs::write(&gecici, baytlar).map_err(|hata| Hata::disk(&gecici, &hata))?;
    std::fs::rename(&gecici, hedef).map_err(|hata| {
        let _ = std::fs::remove_file(&gecici);
        Hata::disk(hedef, &hata)
    })
}

#[cfg(test)]
// unwrap/expect yalnizca testlerde ve gerekçeyle kullanilabilir (WORKER_CONTRACT.md § 4.2).
// Bir testte hata durumu testin kendisidir; sessizce yutulmamalidir.
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    struct GeciciProje {
        yol: PathBuf,
    }

    impl GeciciProje {
        fn yeni(etiket: &str) -> std::io::Result<Self> {
            let yol = std::env::temp_dir().join(format!(
                "siteturk-uretim-{}-{}",
                etiket,
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&yol);
            std::fs::create_dir_all(&yol)?;
            Ok(Self { yol })
        }

        fn yaz(&self, goreli: &str, icerik: &str) -> std::io::Result<PathBuf> {
            let tam = self.yol.join(goreli);
            if let Some(ust) = tam.parent() {
                std::fs::create_dir_all(ust)?;
            }
            std::fs::write(&tam, icerik)?;
            Ok(tam)
        }

        fn yaz_yapilandirma(&self) -> std::io::Result<()> {
            let metin = r#"{
  "kaynak": "icerik",
  "cikti": "dist",
  "sablon": "sablonlar/sayfa.html",
  "varlik": "static",
  "adres": "https://ornek.example"
}"#;
            self.yaz("siteturk.json", metin).map(|_| ())
        }

        fn sablon(&self) -> std::io::Result<()> {
            self.yaz(
                "sablonlar/sayfa.html",
                "<!DOCTYPE html><html><head><title>{{ sayfa.baslik }}</title>\
                 <link rel=\"stylesheet\" href=\"{{ varlik(\"sayfa.css\") }}\"></head>\
                 <body><h1>{{ sayfa.baslik }}</h1>{{ sayfa.govde | ham }}\
                 <ul>{% for s in tumSayfalar %}<li><a href=\"{{ s.adres }}\">{{ s.baslik }}</a></li>{% endfor %}</ul>\
                 </body></html>\n",
            )
            .map(|_| ())
        }

        fn yapilandirma(&self) -> Yapilandirma {
            Yapilandirma::yukle(&self.yol).unwrap_or_default()
        }
    }

    impl Drop for GeciciProje {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.yol);
        }
    }

    fn hazir_proje(etiket: &str) -> GeciciProje {
        let proje = GeciciProje::yeni(etiket).unwrap();
        proje.yaz_yapilandirma().unwrap();
        proje.sablon().unwrap();
        proje
    }

    #[test]
    fn tek_sayfa_uretir() {
        let proje = hazir_proje("tek");
        proje
            .yaz(
                "icerik/index.md",
                "---\nbaslik: Ana sayfa\n---\n\nMerhaba **dunya**.\n",
            )
            .unwrap();
        let sonuc = uret(
            &proje.yol,
            &proje.yapilandirma(),
            UretimSecenekleri::default(),
        )
        .unwrap();
        assert_eq!(sonuc.rapor.yeniden_uretilen, 1);
        assert_eq!(sonuc.rapor.korunan, 0);
        let html = std::fs::read_to_string(proje.yol.join("dist/index.html")).unwrap();
        assert!(html.contains("<h1>Ana sayfa</h1>"));
        assert!(html.contains("<strong>dunya</strong>"));
    }

    #[test]
    fn sitemap_ve_robots_uretir() {
        let proje = hazir_proje("sitemap");
        proje
            .yaz("icerik/index.md", "---\nbaslik: Ana\n---\nGercek.")
            .unwrap();
        uret(
            &proje.yol,
            &proje.yapilandirma(),
            UretimSecenekleri::default(),
        )
        .unwrap();
        let harita = std::fs::read_to_string(proje.yol.join("dist/sitemap.xml")).unwrap();
        assert!(
            harita.contains("<loc>https://ornek.example/</loc>"),
            "{harita}"
        );
        let robots = std::fs::read_to_string(proje.yol.join("dist/robots.txt")).unwrap();
        assert!(robots.contains("Sitemap: https://ornek.example/sitemap.xml"));
    }

    #[test]
    fn ikinci_uretim_degismeyen_dosyalari_korur() {
        let proje = hazir_proje("kismi");
        proje
            .yaz("icerik/index.md", "---\nbaslik: Ana\n---\nBir.")
            .unwrap();
        proje
            .yaz("icerik/blog/yazi.md", "---\nbaslik: Yazi\n---\nIki.")
            .unwrap();
        uret(
            &proje.yol,
            &proje.yapilandirma(),
            UretimSecenekleri::default(),
        )
        .unwrap();
        let ikinci = uret(
            &proje.yol,
            &proje.yapilandirma(),
            UretimSecenekleri::default(),
        )
        .unwrap();
        assert_eq!(ikinci.rapor.yeniden_uretilen, 0);
        assert_eq!(ikinci.rapor.korunan, 2);
    }

    #[test]
    fn tam_uretim_herseyi_yeniden_yazar() {
        let proje = hazir_proje("tam");
        proje
            .yaz("icerik/index.md", "---\nbaslik: Ana\n---\nBir.")
            .unwrap();
        uret(
            &proje.yol,
            &proje.yapilandirma(),
            UretimSecenekleri::default(),
        )
        .unwrap();
        let ikinci = uret(
            &proje.yol,
            &proje.yapilandirma(),
            UretimSecenekleri { tam_uretim: true },
        )
        .unwrap();
        assert_eq!(ikinci.rapor.yeniden_uretilen, 1);
        assert_eq!(ikinci.rapor.korunan, 0);
    }

    #[test]
    fn taslak_uretmez_ve_robots_a_yarar() {
        let proje = hazir_proje("taslak");
        proje
            .yaz("icerik/index.md", "---\nbaslik: Ana\n---\nBir.")
            .unwrap();
        proje
            .yaz(
                "icerik/taslak/not.md",
                "---\nbaslik: Taslak\ntaslak: true\n---\nGizli.\n",
            )
            .unwrap();
        let sonuc = uret(
            &proje.yol,
            &proje.yapilandirma(),
            UretimSecenekleri::default(),
        )
        .unwrap();
        assert_eq!(sonuc.rapor.taslak, 1);
        assert!(!proje.yol.join("dist/taslak/not/index.html").exists());
        let robots = std::fs::read_to_string(proje.yol.join("dist/robots.txt")).unwrap();
        assert!(robots.contains("Disallow: /taslak/not/"), "{robots}");
    }

    #[test]
    fn varlik_kopyalanir_ve_baglanti_yeniden_yazilir() {
        let proje = hazir_proje("varlik");
        proje
            .yaz("icerik/index.md", "---\nbaslik: Ana\n---\nBir.")
            .unwrap();
        proje.yaz("static/sayfa.css", "body{color:red}").unwrap();
        let sonuc = uret(
            &proje.yol,
            &proje.yapilandirma(),
            UretimSecenekleri::default(),
        )
        .unwrap();
        assert_eq!(sonuc.rapor.varlik, 1);
        let html = std::fs::read_to_string(proje.yol.join("dist/index.html")).unwrap();
        assert!(html.contains("/static/sayfa."), "{html}");
        assert!(html.contains(".css\""));
    }

    #[test]
    fn bozuk_on_bilgi_uretimi_durdurur() {
        let proje = hazir_proje("bozuk");
        proje
            .yaz(
                "icerik/index.md",
                "---\nbaslik: T\nbilinmeyen: 1\n---\nGovde",
            )
            .unwrap();
        let hata = uret(
            &proje.yol,
            &proje.yapilandirma(),
            UretimSecenekleri::default(),
        )
        .unwrap_err();
        assert!(hata.to_string().contains("tanimsiz alan"), "{hata}");
    }

    #[test]
    fn sablon_dosyasi_yoksa_hata_verir() {
        let proje = GeciciProje::yeni("sablon-yok").unwrap();
        proje.yaz_yapilandirma().unwrap();
        proje
            .yaz("icerik/index.md", "---\nbaslik: T\n---\nG")
            .unwrap();
        let hata = uret(
            &proje.yol,
            &proje.yapilandirma(),
            UretimSecenekleri::default(),
        )
        .unwrap_err();
        assert!(
            hata.to_string().contains("sablon dosyasi bulunamadi"),
            "{hata}"
        );
    }

    #[test]
    fn icerik_klasoru_yoksa_hata_verir() {
        let proje = hazir_proje("yok");
        let hata = uret(
            &proje.yol,
            &proje.yapilandirma(),
            UretimSecenekleri::default(),
        )
        .unwrap_err();
        assert!(
            hata.to_string().contains("icerik klasoru bulunamadi"),
            "{hata}"
        );
    }

    #[test]
    fn olu_dosya_temizlenir() {
        let proje = hazir_proje("olu");
        proje
            .yaz("icerik/index.md", "---\nbaslik: Ana\n---\nBir.")
            .unwrap();
        proje
            .yaz("icerik/silincek.md", "---\nbaslik: Gecici\n---\nX.")
            .unwrap();
        uret(
            &proje.yol,
            &proje.yapilandirma(),
            UretimSecenekleri::default(),
        )
        .unwrap();
        assert!(proje.yol.join("dist/silincek/index.html").exists());
        std::fs::remove_file(proje.yol.join("icerik/silincek.md")).unwrap();
        let sonuc = uret(
            &proje.yol,
            &proje.yapilandirma(),
            UretimSecenekleri::default(),
        )
        .unwrap();
        assert_eq!(sonuc.rapor.silinen, 1);
        assert!(!proje.yol.join("dist/silincek/index.html").exists());
    }

    #[test]
    fn markdown_uyarilari_toplanir() {
        let proje = hazir_proje("uyari");
        proje
            .yaz(
                "icerik/index.md",
                "---\nbaslik: T\n---\n```rust\nacik kalan\n",
            )
            .unwrap();
        let sonuc = uret(
            &proje.yol,
            &proje.yapilandirma(),
            UretimSecenekleri::default(),
        )
        .unwrap();
        assert!(sonuc.rapor.uyari >= 1);
        assert!(sonuc.uyarilar.iter().any(|u| u.contains("kapanmayan")));
    }

    #[test]
    fn slug_on_bilgisi_adresi_geresalar() {
        let proje = hazir_proje("slug");
        proje
            .yaz("icerik/index.md", "---\nbaslik: T\n---\nBir.")
            .unwrap();
        proje
            .yaz(
                "icerik/eskiler/yazi.md",
                "---\nbaslik: Y\nslug: yeni-adres\n---\nİki.",
            )
            .unwrap();
        uret(
            &proje.yol,
            &proje.yapilandirma(),
            UretimSecenekleri::default(),
        )
        .unwrap();
        assert!(proje.yol.join("dist/yeni-adres/index.html").exists());
        let harita = std::fs::read_to_string(proje.yol.join("dist/sitemap.xml")).unwrap();
        assert!(harita.contains("/yeni-adres/"), "{harita}");
    }

    #[test]
    fn atomik_yazma_gecici_dosya_birakmaz() {
        let proje = hazir_proje("atomik");
        let hedef = proje.yol.join("dist/ornek.txt");
        yaz_atomik(&hedef, b"icerik").unwrap();
        assert_eq!(std::fs::read_to_string(&hedef).unwrap(), "icerik");
        let gecici = proje
            .yol
            .join("dist")
            .join(format!("{GECICI_ONEK}ornek.txt"));
        assert!(!gecici.exists());
    }

    #[test]
    fn onbellek_bozuksa_tam_uretim_yapar() {
        let proje = hazir_proje("bozuk-onbellek");
        proje
            .yaz("icerik/index.md", "---\nbaslik: T\n---\nBir.")
            .unwrap();
        proje
            .yaz(".siteturk-onbellek.json", "{bozuk onbellek")
            .unwrap();
        let sonuc = uret(
            &proje.yol,
            &proje.yapilandirma(),
            UretimSecenekleri::default(),
        )
        .unwrap();
        assert_eq!(sonuc.rapor.yeniden_uretilen, 1);
    }

    #[test]
    fn ayni_adres_iki_dosyada_uretimi_durdurur() {
        let proje = hazir_proje("cakisma");
        proje
            .yaz("icerik/a.md", "---\nbaslik: A\n---\nBir.")
            .unwrap();
        proje
            .yaz("icerik/b.md", "---\nbaslik: B\nslug: a\n---\nİki.")
            .unwrap();
        let hata = uret(
            &proje.yol,
            &proje.yapilandirma(),
            UretimSecenekleri::default(),
        )
        .unwrap_err();
        assert!(hata.to_string().contains("ayni adresi"), "{hata}");
    }

    #[test]
    fn siralama_alanina_gore_siralama_etkisi() {
        let proje = hazir_proje("siralama");
        proje
            .yaz("icerik/index.md", "---\nbaslik: A\nsiralama: 1\n---\nBir.")
            .unwrap();
        proje
            .yaz("icerik/b.md", "---\nbaslik: B\nsiralama: 2\n---\nİki.")
            .unwrap();
        let sonuc = uret(
            &proje.yol,
            &proje.yapilandirma(),
            UretimSecenekleri::default(),
        )
        .unwrap();
        assert_eq!(sonuc.adresler, vec!["/", "/b/"]);
    }
}
