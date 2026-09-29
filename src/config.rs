//! `siteturk.json` yapılandırma şeması ve yükleyici.
//!
//! Yapılandırma tek dosyadır ve **yalnızca tanımlı alanları** kabul eder.
//! Tanımsız alan hata üretir; bu, "sessizce yutulan ayar" sınıfındaki hataları
//! ilk denemede görünür kılar (rapor § 06, "Yapılandırma").

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::bom_kaldir;
use crate::error::Hata;

/// Proje yapılandırması.
///
/// Tüm alanların varsayılanı vardır; `siteturk.json` olmadan da çalışılabilir.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Yapilandirma {
    /// Markdown içerik klasörü (yapılandırma dosyasına göreli).
    #[serde(default = "varsayilan_kaynak")]
    pub kaynak: PathBuf,
    /// Üretilen sitenin yazılacağı klasör.
    #[serde(default = "varsayilan_cikti")]
    pub cikti: PathBuf,
    /// Sayfa şablonunun yolu.
    #[serde(default = "varsayilan_sablon")]
    pub sablon: PathBuf,
    /// Varlık (statik dosya) klasörü.
    #[serde(default = "varsayilan_varlik")]
    pub varlik: PathBuf,
    /// Sitenin mutlak temel adresi; `sitemap.xml` ve `robots.txt` bunu kullanır.
    #[serde(default = "varsayilan_adres")]
    pub adres: String,
    /// `sitemap.xml` üretilsin mi.
    #[serde(default = "varsayilan_dogru")]
    pub sitemap: bool,
    /// `robots.txt` üretilsin mi.
    #[serde(default = "varsayilan_dogru")]
    pub robots: bool,
    /// `taslak: true` sayfaları da üret (varsayılan: üretme).
    #[serde(default)]
    pub taslaklari_yayinla: bool,
    /// Varlık adlarına içerik imzası eklensin mi.
    #[serde(default = "varsayilan_dogru")]
    pub imzali_varlik: bool,
    /// Varlıklarda boşluk kısaltma uygulansın mı.
    #[serde(default = "varsayilan_dogru")]
    pub kucult: bool,
}

fn varsayilan_kaynak() -> PathBuf {
    PathBuf::from("icerik")
}

fn varsayilan_cikti() -> PathBuf {
    PathBuf::from("dist")
}

fn varsayilan_sablon() -> PathBuf {
    PathBuf::from("sablonlar/sayfa.html")
}

fn varsayilan_varlik() -> PathBuf {
    PathBuf::from("static")
}

fn varsayilan_adres() -> String {
    "https://ornek.example".to_string()
}

fn varsayilan_dogru() -> bool {
    true
}

impl Default for Yapilandirma {
    fn default() -> Self {
        Yapilandirma {
            kaynak: varsayilan_kaynak(),
            cikti: varsayilan_cikti(),
            sablon: varsayilan_sablon(),
            varlik: varsayilan_varlik(),
            adres: varsayilan_adres(),
            sitemap: true,
            robots: true,
            taslaklari_yayinla: false,
            imzali_varlik: true,
            kucult: true,
        }
    }
}

impl Yapilandirma {
    /// Yapılandırma dosyasının öntanılan adı.
    pub const DOSYA_ADI: &'static str = "siteturk.json";

    /// Verilen kök klasörde `siteturk.json` arar; yoksa varsayılanları döner.
    ///
    /// Dosya yoksa hata üretilmez: raporun "yapılandırma tek dosya; yoksa
    /// varsayılanlar" kararı böyledir.
    pub fn yukle(kok: &Path) -> Result<Self, Hata> {
        let yol = kok.join(Yapilandirma::DOSYA_ADI);
        Yapilandirma::yukle_dosyadan(&yol)
    }

    /// Belirtilen dosyadan yapılandırmayı okur.
    pub fn yukle_dosyadan(yol: &Path) -> Result<Self, Hata> {
        let metin = match std::fs::read_to_string(yol) {
            Ok(metin) => metin,
            Err(kaynak) if kaynak.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Yapilandirma::default());
            }
            Err(kaynak) => return Err(Hata::disk(yol, &kaynak)),
        };
        Yapilandirma::ayikla_metin(yol, bom_kaldir(&metin))
    }

    /// Yapılandırma metnini ayrıştırır.
    ///
    /// # Hatalar
    ///
    /// Bozuk JSON, tanımsız alan ya da eksik zorunlu değer `Hata::Yapilandirma` üretir.
    pub fn ayikla_metin(yol: &Path, metin: &str) -> Result<Self, Hata> {
        let ayiklanmis: Self = serde_json::from_str(metin).map_err(|hata| Hata::Yapilandirma {
            yol: yol.display().to_string(),
            ayrinti: hata.to_string(),
        })?;
        ayiklanmis.dogrula(yol)
    }

    /// Anlamsal denetim yapar.
    pub fn dogrula(&self, yol: &Path) -> Result<Self, Hata> {
        let hata = |ayrinti: String| Hata::Yapilandirma {
            yol: yol.display().to_string(),
            ayrinti,
        };
        if self.adres.trim().is_empty() {
            return Err(hata("`adres` bos birakilamaz".to_string()));
        }
        if !self.adres.starts_with("http://") && !self.adres.starts_with("https://") {
            return Err(hata(format!(
                "`adres` http:// ya da https:// ile baslamali, verilen: `{}`",
                self.adres
            )));
        }
        for (ad, deger) in [
            ("kaynak", &self.kaynak),
            ("cikti", &self.cikti),
            ("sablon", &self.sablon),
            ("varlik", &self.varlik),
        ] {
            if deger.as_os_str().is_empty() {
                return Err(hata(format!("`{ad}` bos birakilamaz")));
            }
            if deger.is_absolute() {
                return Err(hata(format!(
                    "`{ad}` mutlak yol olamaz; proje kokune goreli yazilmali: `{}`",
                    deger.display()
                )));
            }
            if deger
                .components()
                .any(|p| matches!(p, std::path::Component::ParentDir))
            {
                return Err(hata(format!(
                    "`{ad}` ust dizin (`..`) iceremez: `{}`",
                    deger.display()
                )));
            }
        }
        if self.kaynak == self.cikti {
            return Err(hata(
                "`kaynak` ve `cikti` ayni olamaz; uretim kaynak klasorunu yazamaz".to_string(),
            ));
        }
        Ok(self.clone())
    }

    /// Yapılandırmayı JSON olarak okunabilir biçimde yazar.
    pub fn yaz(&self, yol: &Path) -> Result<(), Hata> {
        let metin = serde_json::to_string_pretty(self).map_err(|hata| Hata::Yapilandirma {
            yol: yol.display().to_string(),
            ayrinti: hata.to_string(),
        })?;
        if let Some(ust) = yol.parent() {
            std::fs::create_dir_all(ust).map_err(|kaynak| Hata::disk(ust, &kaynak))?;
        }
        std::fs::write(yol, format!("{metin}\n")).map_err(|kaynak| Hata::disk(yol, &kaynak))
    }

    /// Şablonlara verilen temel bağlamı üretir.
    pub fn site_baglami(&self) -> crate::template::Deger {
        let mut harita = BTreeMap::new();
        harita.insert(
            "adres".to_string(),
            crate::template::Deger::metin(&self.adres),
        );
        harita.insert(
            "sitemap".to_string(),
            crate::template::Deger::Sayi(i64::from(self.sitemap)),
        );
        harita.insert(
            "robots".to_string(),
            crate::template::Deger::Sayi(i64::from(self.robots)),
        );
        crate::template::Deger::Harita(harita)
    }
}

#[cfg(test)]
// unwrap/expect yalnizca testlerde ve gerekçeyle kullanilabilir (WORKER_CONTRACT.md § 4.2).
// Bir testte hata durumu testin kendisidir; sessizce yutulmamalidir.
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn varsayilanlar_beceridir() {
        let varsayilan = Yapilandirma::default();
        assert_eq!(varsayilan.kaynak, PathBuf::from("icerik"));
        assert_eq!(varsayilan.cikti, PathBuf::from("dist"));
        assert!(varsayilan.sitemap);
        assert!(!varsayilan.taslaklari_yayinla);
    }

    #[test]
    fn dosya_yoksa_varsayilan_doner() {
        let kok = std::env::temp_dir().join("siteturk-yok-boyle-bir-dosya.json");
        let sonuc = Yapilandirma::yukle_dosyadan(&kok);
        assert_eq!(sonuc.unwrap(), Yapilandirma::default());
    }

    #[test]
    fn gecerli_json_okunur() {
        let metin = r#"{"adres": "https://site.example", "kucult": false}"#;
        let yol = Path::new("siteturk.json");
        let sonuc = Yapilandirma::ayikla_metin(yol, metin).unwrap();
        assert_eq!(sonuc.adres, "https://site.example");
        assert!(!sonuc.kucult);
        assert_eq!(sonuc.cikti, PathBuf::from("dist"));
    }

    #[test]
    fn bozuk_json_hata_verir() {
        let hata = Yapilandirma::ayikla_metin(Path::new("siteturk.json"), "{ bozuk").unwrap_err();
        assert!(hata.to_string().contains("yapilandirma hatasi"), "{hata}");
    }

    #[test]
    fn tanimsiz_alan_hata_verir() {
        let metin = r#"{"bilinmeyen": 1}"#;
        let hata = Yapilandirma::ayikla_metin(Path::new("siteturk.json"), metin).unwrap_err();
        assert!(hata.to_string().contains("unknown field"), "{hata}");
    }

    #[test]
    fn mutlak_yol_hata_verir() {
        let metin = r#"{"kaynak": "C:/gizli"}"#;
        let hata = Yapilandirma::ayikla_metin(Path::new("siteturk.json"), metin).unwrap_err();
        assert!(hata.to_string().contains("mutlak yol"), "{hata}");
    }

    #[test]
    fn ust_dizin_yolu_hata_verir() {
        let metin = r#"{"kaynak": "../kacis"}"#;
        let hata = Yapilandirma::ayikla_metin(Path::new("siteturk.json"), metin).unwrap_err();
        assert!(hata.to_string().contains("ust dizin"), "{hata}");
    }

    #[test]
    fn protokolsuz_adres_hata_verir() {
        let metin = r#"{"adres": "site.example"}"#;
        let hata = Yapilandirma::ayikla_metin(Path::new("siteturk.json"), metin).unwrap_err();
        assert!(hata.to_string().contains("http://"), "{hata}");
    }

    #[test]
    fn ayni_kaynak_cikti_hata_verir() {
        let metin = r#"{"kaynak": "a", "cikti": "a"}"#;
        let hata = Yapilandirma::ayikla_metin(Path::new("siteturk.json"), metin).unwrap_err();
        assert!(hata.to_string().contains("ayni olamaz"), "{hata}");
    }

    #[test]
    fn site_baglami_uretilir() {
        let baglam = Yapilandirma::default().site_baglami();
        assert_eq!(
            baglam.ara("adres").unwrap().metin_kacisli(),
            "https://ornek.example"
        );
    }

    #[test]
    fn yazma_okuma_gidis_donus_yapar() {
        let dizin = std::env::temp_dir().join("siteturk-yapilandirma-testi");
        let _ = std::fs::remove_dir_all(&dizin);
        let yol = dizin.join("siteturk.json");
        let ornek = Yapilandirma::default();
        ornek.yaz(&yol).unwrap();
        let okunan = Yapilandirma::yukle_dosyadan(&yol).unwrap();
        assert_eq!(ornek, okunan);
        let _ = std::fs::remove_dir_all(&dizin);
    }
}
