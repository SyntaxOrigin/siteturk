//! Merkezî hata tipi ve `std::error::Error` uygulaması.
//!
//! Bu modül hata *türlerini* tanımlar; hiçbir hata üretmez. `thiserror` gibi bir
//! türetilmiş hata kütüphanesi bağımlılık politikası gereği kullanılamaz
//! (bkz. `WORKER_CONTRACT.md` § 3.2), bu yüzden `Display` elle yazılmıştır.

use std::fmt;
use std::path::Path;

/// Projenin ürettiği tüm hataların ortak tipi.
///
/// Her varyant kullanıcıya **dosya ve satır** bilgisini taşıyacak biçimde
/// tasarlanmıştır; şablon dili ve Markdown ayrıştırıcısındaki hataların
/// "hangi satırı düzelteceğim" sorusunu yanıtlaması beklenir.
#[derive(Debug)]
#[non_exhaustive]
pub enum Hata {
    /// Yapılandırma dosyası okunamadı, ayrıştırılamadı veya alan doğrulaması
    /// başarısız oldu.
    Yapilandirma {
        /// Hatanın geçtiği dosya (yapılandırma dosyası yolu).
        yol: String,
        /// Kullanıcıya gösterilecek açıklama.
        ayrinti: String,
    },
    /// Ön bilgi (front matter) bloğu bozuk: kapanmamış `---`, eksik iki nokta
    /// veya tanımsız alan adı.
    OnBilgi {
        /// Hatanın geçtiği içerik dosyası.
        dosya: String,
        /// Hatanın geçtiği satır numarası (1 tabanlı).
        satir: usize,
        /// Kullanıcıya gösterilecek açıklama.
        mesaj: String,
    },
    /// Markdown gövdesi ayrıştırılırken üretilen hata.
    Markdown {
        /// Hatanın geçtiği içerik dosyası.
        dosya: String,
        /// Hatanın geçtiği satır numarası (1 tabanlı).
        satir: usize,
        /// Kullanıcıya gösterilecek açıklama.
        mesaj: String,
    },
    /// Şablon söz dizimi veya şablon çalıştırma hatası.
    Sablon {
        /// Hatanın geçtiği şablon dosyası.
        dosya: String,
        /// Hatanın geçtiği satır numarası (1 tabanlı).
        satir: usize,
        /// Kullanıcıya gösterilecek açıklama.
        mesaj: String,
    },
    /// Disk okuma/yazma hatası.
    Disk {
        /// İşlem yapılan yol.
        yol: String,
        /// `std::io::Error` açıklaması.
        ayrinti: String,
    },
    /// Canlı yenileme sunucusunda oluşan hata (örneğin port bağlama).
    Sunucu {
        /// Kullanıcıya gösterilecek açıklama.
        ayrinti: String,
    },
    /// Komut satırı kullanım hatası.
    Kullanim {
        /// Kullanıcıya gösterilecek açıklama.
        ayrinti: String,
    },
}

impl Hata {
    /// Disk hatası üretir; `io::Error` ayrıntısı metne çevrilir.
    pub fn disk(yol: &Path, kaynak: &std::io::Error) -> Self {
        Hata::Disk {
            yol: yol.display().to_string(),
            ayrinti: kaynak.to_string(),
        }
    }

    /// Ön bilgi hatası üretir.
    pub fn on_bilgi(dosya: &str, satir: usize, mesaj: impl Into<String>) -> Self {
        Hata::OnBilgi {
            dosya: dosya.to_string(),
            satir,
            mesaj: mesaj.into(),
        }
    }

    /// Şablon hatası üretir.
    pub fn sablon(dosya: &str, satir: usize, mesaj: impl Into<String>) -> Self {
        Hata::Sablon {
            dosya: dosya.to_string(),
            satir,
            mesaj: mesaj.into(),
        }
    }
}

impl fmt::Display for Hata {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Hata::Yapilandirma { yol, ayrinti } => {
                write!(f, "yapilandirma hatasi ({yol}): {ayrinti}")
            }
            Hata::OnBilgi {
                dosya,
                satir,
                mesaj,
            } => write!(f, "on bilgi hatasi ({dosya}:{satir}): {mesaj}"),
            Hata::Markdown {
                dosya,
                satir,
                mesaj,
            } => write!(f, "markdown hatasi ({dosya}:{satir}): {mesaj}"),
            Hata::Sablon {
                dosya,
                satir,
                mesaj,
            } => write!(f, "sablon hatasi ({dosya}:{satir}): {mesaj}"),
            Hata::Disk { yol, ayrinti } => write!(f, "disk hatasi ({yol}): {ayrinti}"),
            Hata::Sunucu { ayrinti } => write!(f, "sunucu hatasi: {ayrinti}"),
            Hata::Kullanim { ayrinti } => write!(f, "kullanim hatasi: {ayrinti}"),
        }
    }
}

impl std::error::Error for Hata {}

#[cfg(test)]
// unwrap/expect yalnizca testlerde ve gerekçeyle kullanilabilir (WORKER_CONTRACT.md § 4.2).
// Bir testte hata durumu testin kendisidir; sessizce yutulmamalidir.
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn disk_hatasi_yol_ve_ayrinti_tasir() {
        let kaynak = std::io::Error::new(std::io::ErrorKind::NotFound, "dosya yok");
        let hata = Hata::disk(Path::new("icerik/a.md"), &kaynak);
        let metin = hata.to_string();
        assert!(
            metin.contains("icerik/a.md"),
            "yol metinde geçmeli: {metin}"
        );
        assert!(
            metin.contains("dosya yok"),
            "ayrinti metinde geçmeli: {metin}"
        );
    }

    #[test]
    fn on_bilgi_hatasi_dosya_ve_satir_gosterir() {
        let hata = Hata::on_bilgi("blog/yazi.md", 7, "kapanmayan blok");
        assert_eq!(
            hata.to_string(),
            "on bilgi hatasi (blog/yazi.md:7): kapanmayan blok"
        );
    }

    #[test]
    fn sablon_hatasi_dosya_ve_satir_gosterir() {
        let hata = Hata::sablon("sablon.html", 12, "endif eksik");
        assert_eq!(
            hata.to_string(),
            "sablon hatasi (sablon.html:12): endif eksik"
        );
    }

    #[test]
    fn hata_tipi_std_error_uyumludur() {
        let hata = Hata::Kullanim {
            ayrinti: "bilinmeyen komut".to_string(),
        };
        let metin = format!("{}", hata);
        assert_eq!(metin, "kullanim hatasi: bilinmeyen komut");
        let _: &dyn std::error::Error = &hata;
    }
}
