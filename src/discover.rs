//! Dosya ağacı keşfi, adlandırma kuralı ve önbellek anahtarı üretimi.
//!
//! Keşif **kendi özyinelemeli `read_dir` gezintisiyle** yapılır; `walkdir`
//! bağımlılığı yasaktır (`WORKER_CONTRACT.md` § 3.2-F).
//!
//! Adlandırma kuralı (yapılandırılamaz, tek ve öngörülebilir):
//!
//! | Kaynak dosya | Üretilen dosya | Adres |
//! |---|---|---|
//! | `icerik/index.md` | `index.html` | `/` |
//! | `icerik/hakkimizda.md` | `hakkimizda/index.html` | `/hakkimizda/` |
//! | `icerik/blog/yazi.md` | `blog/yazi/index.html` | `/blog/yazi/` |
//!
//! Kural, "temiz adres" (uzantısız) adres üretir ve her sayfanın çıktısı bir
//! klasörün `index.html` dosyası olur. Dağıtım klasörü hiçbir sunucu ayarı
//! olmadan çalışır (rapor § 03, senaryo S6).

use std::path::{Component, Path, PathBuf};

use crate::error::Hata;

/// Keşfedilen tek bir içerik dosyasının özeti.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IcerikDosyasi {
    /// Kaynak dosyanın tam yolu.
    pub kaynak: PathBuf,
    /// Kaynak köke göreli yol (`blog/yazi.md`).
    pub goreli: String,
    /// Üretilecek dosyanın çıktı köküne göreli yolu (`blog/yazi/index.html`).
    pub cikti: String,
    /// Sitenin adres çubuğunda görüneceği yol (`/blog/yazi/`).
    pub adres: String,
    /// Değişiklik algılama imzası: `mtime` nanosaniye + boyut.
    pub imza: Imza,
}

/// Bir dosyanın değişip değişmediğini belirleyen imza.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Imza {
    /// Dosyanın son değiştirilme zamanı (UNIX nanosaniye, `u128`).
    pub mtime: u128,
    /// Dosyanın bayt cinsinden boyutu (`u64`).
    pub boyut: u64,
}

impl Imza {
    /// Dosya sistemi bilgilerinden imza üretir.
    pub fn dosyadan(yol: &Path) -> Result<Self, Hata> {
        let meta = std::fs::metadata(yol).map_err(|kaynak| Hata::disk(yol, &kaynak))?;
        Imza::metadan(&meta).ok_or_else(|| {
            Hata::disk(
                yol,
                &std::io::Error::other("dosya sistemi degistirme zamani vermedi"),
            )
        })
    }

    /// `Metadata` değerinden imza üretir.
    pub fn metadan(meta: &std::fs::Metadata) -> Option<Self> {
        let mtime = meta.modified().ok()?;
        let nanos = mtime
            .duration_since(std::time::UNIX_EPOCH)
            .map(|s| s.as_nanos())
            .unwrap_or(0);
        Some(Imza {
            mtime: nanos,
            boyut: meta.len(),
        })
    }
}

/// Özyinelemeli gezintiyle tüm Markdown dosyalarını bulur.
///
/// Sonuç **yol sırasına göre** döner; "aynı girdiden aynı çıktı" ilkesi için
/// dosya sistemi sırasına güvenilmez.
pub fn markdown_dosyalarini_bul(kaynak: &Path) -> Result<Vec<IcerikDosyasi>, Hata> {
    let mut ham: Vec<(String, PathBuf)> = Vec::new();
    gez(kaynak, kaynak, &mut ham, 0)?;
    ham.sort_by(|a, b| a.0.cmp(&b.0));

    let mut sonuc = Vec::with_capacity(ham.len());
    for (goreli, tam) in ham {
        let cikti = cikti_yolu(&goreli);
        let adres = adres_yolu(&goreli);
        let imza = Imza::dosyadan(&tam)?;
        sonuc.push(IcerikDosyasi {
            kaynak: tam,
            goreli,
            cikti,
            adres,
            imza,
        });
    }
    Ok(sonuc)
}

/// Verilen dosya için adlandırma kuralını uygular.
pub fn adlandir(goreli: &str) -> (String, String) {
    (cikti_yolu(goreli), adres_yolu(goreli))
}

/// `blog/yazi.md` → `blog/yazi/index.html`, `blog/index.md` → `blog/index.html`.
///
/// Son segment `index` ise çıktı dosyası zaten adı taşır; aksi hâlde dosya adı
/// bir klasör adına dönüşür.
fn cikti_yolu(goreli: &str) -> String {
    let govde = govde_yolu(goreli);
    let (dizin, ad) = dizin_ve_ad(govde);
    if ad == "index" {
        if dizin.is_empty() {
            "index.html".to_string()
        } else {
            format!("{dizin}/index.html")
        }
    } else {
        format!("{govde}/index.html")
    }
}

/// `blog/yazi.md` → `/blog/yazi/`, `blog/index.md` → `/blog/`, `index.md` → `/`.
fn adres_yolu(goreli: &str) -> String {
    let govde = govde_yolu(goreli);
    let (dizin, ad) = dizin_ve_ad(govde);
    if ad == "index" {
        if dizin.is_empty() {
            "/".to_string()
        } else {
            format!("/{dizin}/")
        }
    } else {
        format!("/{govde}/")
    }
}

/// Uzantıyı soyulmuş gövde.
fn govde_yolu(goreli: &str) -> &str {
    goreli.strip_suffix(".md").unwrap_or(goreli)
}

/// Gövdeyi `(dizin, ad)` olarak böler.
fn dizin_ve_ad(govde: &str) -> (&str, &str) {
    match govde.rfind('/') {
        Some(konum) => (&govde[..konum], &govde[konum + 1..]),
        None => ("", govde),
    }
}

/// `index` adlı dosyaların adresi üst klasörün adresine düşer.
pub fn adresi_normalleştir(adres: &str) -> String {
    if adres.ends_with("/index/") {
        adres.trim_end_matches("index/").to_string()
    } else {
        adres.to_string()
    }
}

/// Bir dizini özyinelemeli olarak gezer ve `.md` dosyalarını toplar.
fn gez(
    kok: &Path,
    simdi: &Path,
    toplanan: &mut Vec<(String, PathBuf)>,
    derinlik: usize,
) -> Result<(), Hata> {
    if derinlik > 32 {
        return Err(Hata::Kullanim {
            ayrinti: format!("icerik klasoru cok derin (32 seviye): {}", simdi.display()),
        });
    }
    let girdiler = std::fs::read_dir(simdi).map_err(|kaynak| Hata::disk(simdi, &kaynak))?;
    for girdi in girdiler {
        let girdi = girdi.map_err(|kaynak| Hata::disk(simdi, &kaynak))?;
        let yol = girdi.path();
        let ad = girdi.file_name();
        let ad = ad.to_string_lossy().to_string();
        if ad.starts_with('.') {
            continue;
        }
        let tip = girdi
            .file_type()
            .map_err(|kaynak| Hata::disk(&yol, &kaynak))?;
        if tip.is_dir() {
            gez(kok, &yol, toplanan, derinlik + 1)?;
        } else if tip.is_file() && ad.to_ascii_lowercase().ends_with(".md") {
            let goreli = yol
                .strip_prefix(kok)
                .map_err(|_| Hata::Kullanim {
                    ayrinti: format!("dosya kaynak klasorunun disinda: {}", yol.display()),
                })?
                .to_string_lossy()
                .replace('\\', "/");
            toplanan.push((goreli, yol));
        }
    }
    Ok(())
}

/// Bir göreli yolun içerik kökünden kaçmadığını doğrular.
///
/// `..` ya da mutlak yol içeren göreli yollar reddedilir. Bu denetim,
/// sunucu tarafındaki dizin kaçışı denetimiyle aynı ilkeye dayanır.
pub fn guvenli_goreli(yol: &str) -> bool {
    if yol.is_empty() {
        return false;
    }
    !Path::new(yol)
        .components()
        .any(|p| !matches!(p, Component::Normal(_)))
}

#[cfg(test)]
// unwrap/expect yalnizca testlerde ve gerekçeyle kullanilabilir (WORKER_CONTRACT.md § 4.2).
// Bir testte hata durumu testin kendisidir; sessizce yutulmamalidir.
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    struct GeciciDizin {
        yol: PathBuf,
    }

    impl GeciciDizin {
        fn yeni(etiket: &str) -> std::io::Result<Self> {
            let kok =
                std::env::temp_dir().join(format!("siteturk-{}-{}", etiket, std::process::id()));
            let _ = std::fs::remove_dir_all(&kok);
            std::fs::create_dir_all(&kok)?;
            Ok(Self { yol: kok })
        }

        fn yaz(&self, goreli: &str, icerik: &str) -> std::io::Result<PathBuf> {
            let tam = self.yol.join(goreli);
            if let Some(ust) = tam.parent() {
                std::fs::create_dir_all(ust)?;
            }
            std::fs::write(&tam, icerik)?;
            Ok(tam)
        }
    }

    impl Drop for GeciciDizin {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.yol);
        }
    }

    #[test]
    fn kok_dosya_indeks_adresi_verir() {
        let (cikti, adres) = adlandir("index.md");
        assert_eq!(cikti, "index.html");
        assert_eq!(adres, "/");
    }

    #[test]
    fn kok_dosya_adresi_verir() {
        let (cikti, adres) = adlandir("hakkimizda.md");
        assert_eq!(cikti, "hakkimizda/index.html");
        assert_eq!(adres, "/hakkimizda/");
    }

    #[test]
    fn ic_ice_dosya_adresi_verir() {
        let (cikti, adres) = adlandir("blog/2026/yazi.md");
        assert_eq!(cikti, "blog/2026/yazi/index.html");
        assert_eq!(adres, "/blog/2026/yazi/");
    }

    #[test]
    fn klasor_indeks_dosyasi_ust_adresi_verir() {
        let (cikti, adres) = adlandir("blog/index.md");
        assert_eq!(cikti, "blog/index.html");
        assert_eq!(adres, "/blog/");
    }

    #[test]
    fn turkce_dosya_adi_corner() {
        let (cikti, adres) = adlandir("hakkımızda/çözümler.md");
        assert_eq!(cikti, "hakkımızda/çözümler/index.html");
        assert_eq!(adres, "/hakkımızda/çözümler/");
    }

    #[test]
    fn turkce_ve_ingilizce_karismasi_harf_cesidi_dogrulanir() {
        let (cikti, _) = adlandir("hakkımızda/index.md");
        assert!(!cikti.is_ascii());
        let (_, adres) = adlandir("hakkimizda/index.md");
        assert!(adres.is_ascii());
    }

    #[test]
    fn adres_normalizasyonu_uygular() {
        assert_eq!(adresi_normalleştir("/blog/index/"), "/blog/");
        assert_eq!(adresi_normalleştir("/blog/yazi/"), "/blog/yazi/");
    }

    #[test]
    fn guvenli_goreli_dogrulama_yapar() {
        assert!(guvenli_goreli("blog/yazi.md"));
        assert!(!guvenli_goreli("../kacis.md"));
        assert!(!guvenli_goreli("blog/../../kacis.md"));
        assert!(!guvenli_goreli(""));
    }

    #[test]
    fn markdown_dosyalarini_sirali_bulur() {
        let dizin = GeciciDizin::yeni("kesif").unwrap();
        dizin.yaz("b.md", "b").unwrap();
        dizin.yaz("index.md", "a").unwrap();
        dizin.yaz("blog/yazi.md", "c").unwrap();
        dizin.yaz("blog/notlar.txt", "yok sayilir").unwrap();
        let bulunan = markdown_dosyalarini_bul(&dizin.yol).unwrap();
        let adlar: Vec<&str> = bulunan.iter().map(|d| d.goreli.as_str()).collect();
        assert_eq!(adlar, vec!["b.md", "blog/yazi.md", "index.md"]);
    }

    #[test]
    fn gizli_dosyalar_atlanir() {
        let dizin = GeciciDizin::yeni("gizli").unwrap();
        dizin.yaz("gorunur.md", "a").unwrap();
        dizin.yaz(".gizli.md", "b").unwrap();
        let bulunan = markdown_dosyalarini_bul(&dizin.yol).unwrap();
        assert_eq!(bulunan.len(), 1);
        assert_eq!(bulunan[0].goreli, "gorunur.md");
    }

    #[test]
    fn imza_dosya_degisince_degisir() {
        let dizin = GeciciDizin::yeni("imza").unwrap();
        let yol = dizin.yaz("a.md", "kisa").unwrap();
        let ilk = Imza::dosyadan(&yol).unwrap();
        std::fs::write(&yol, "cok daha uzun bir icerik yazildi").unwrap();
        let ikinci = Imza::dosyadan(&yol).unwrap();
        assert_ne!(ilk, ikinci);
        assert!(ikinci.boyut > ilk.boyut);
    }

    #[test]
    fn ayni_imza_ayni_kalir() {
        let dizin = GeciciDizin::yeni("imza2").unwrap();
        let yol = dizin.yaz("a.md", "sabit").unwrap();
        assert_eq!(Imza::dosyadan(&yol).unwrap(), Imza::dosyadan(&yol).unwrap());
    }

    #[test]
    fn dosya_bulunamazsa_disk_hatasi_doner() {
        let yol = std::env::temp_dir().join("siteturk-yok-boyle-bir-dosya-xyz.md");
        let hata = Imza::dosyadan(&yol).unwrap_err();
        assert!(matches!(hata, Hata::Disk { .. }), "{hata}");
    }
}
