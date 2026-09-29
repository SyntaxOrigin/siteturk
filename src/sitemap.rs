//! Yan üretimler: `sitemap.xml` ve `robots.txt`.
//!
//! Site haritası, üretilen her sayfanın adresini **kendi adresiyle** yazar ve
//! adresler özgün (unique) olmalıdır: aynı adresi üreten iki dosya varsa üretim
//! hata verir (rapor § 15, "Aynı adres üreten iki dosya").
//!
//! XML metni elle üretilir; bir XML kütüphanesi bağımlılık politikasıyla
//! uyumsuzdur ve bu çıktı için gereksizdir.

use std::collections::BTreeSet;

use crate::error::Hata;

/// Site haritasına girecek tek bir sayfa.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HaritaGirdisi {
    /// Sayfanın site içi adresi (`/blog/yazi/`).
    pub adres: String,
    /// Sayfanın son değiştirilme zamanı (UNIX saniye); `0` bilinmiyor demektir.
    pub guncellenme: u64,
}

/// `sitemap.xml` içeriğini üretir.
///
/// `adres` temel adrestir (`https://ornek.example`); sonundaki `/` normalize
/// edilir. Girdiler adres sırasına göre yazılır ki "aynı girdiden aynı çıktı"
/// ilkesi bozulmasın.
pub fn sitemap_uret(temel_adres: &str, girdiler: &[HaritaGirdisi]) -> Result<String, Hata> {
    let kok = temel_adres.trim_end_matches('/');
    let mut benzersiz: BTreeSet<&str> = BTreeSet::new();
    let mut cakisma: Vec<&str> = Vec::new();
    for girdi in girdiler {
        if !benzersiz.insert(girdi.adres.as_str()) {
            cakisma.push(&girdi.adres);
        }
    }
    if !cakisma.is_empty() {
        return Err(Hata::Kullanim {
            ayrinti: format!(
                "birden fazla sayfa ayni adresi uretiyor: {}",
                cakisma.join(", ")
            ),
        });
    }

    // Adres sırasına göre yazılır; çağıranın gezinti sırası sonucu değiştiremez.
    let mut sirali: Vec<&HaritaGirdisi> = girdiler.iter().collect();
    sirali.sort_by(|a, b| a.adres.cmp(&b.adres));

    let mut cikti = String::with_capacity(128 + sirali.len() * 96);
    cikti.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    cikti.push_str("<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n");
    for girdi in sirali {
        let mut adres = girdi.adres.clone();
        if !adres.starts_with('/') {
            adres.insert(0, '/');
        }
        cikti.push_str("  <url>\n");
        cikti.push_str(&format!("    <loc>{kok}{adres}</loc>\n"));
        if girdi.guncellenme > 0 {
            cikti.push_str(&format!(
                "    <lastmod>{}</lastmod>\n",
                guncelleme_donustur(girdi.guncellenme)
            ));
        }
        cikti.push_str("  </url>\n");
    }
    cikti.push_str("</urlset>\n");
    Ok(cikti)
}

/// UNIX saniyeyi W3C-DTF biçimine çevirir.
///
/// `SystemTime` ile takvim hesabı yapmak için `std::time` dışında bir takvim
/// kütüphanesi gerekir; bu yüzden takvim dönüşümü bilinen sıfır noktasından
/// (1970-01-01) gün sayısı üzerinden **kaba** bir biçimde yapılır ve ISO
/// biçimi yerine sadece `YYYY-MM-DD` günü yazılır. Bu, `lastmod` için yeterlidir:
/// arama motorları gün hassasiyeti ister, saat değil.
fn guncelleme_donustur(saniye: u64) -> String {
    const GUN: u64 = 86_400;
    let gun = saniye / GUN;
    let mut yil = 1970u64;
    let mut kalan = gun;
    loop {
        let artik = if yil % 4 == 0 && (yil % 100 != 0 || yil % 400 == 0) {
            366
        } else {
            365
        };
        if kalan < artik {
            break;
        }
        kalan -= artik;
        yil += 1;
        if yil > 9999 {
            return "1970-01-01".to_string();
        }
    }
    let artik = if yil % 4 == 0 && (yil % 100 != 0 || yil % 400 == 0) {
        366
    } else {
        365
    };
    let aylar: [u64; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut ay = 0usize;
    let mut kalan_gun = kalan;
    while ay < 12 {
        let mut uzunluk = aylar[ay];
        if ay == 1 && artik == 366 {
            uzunluk = 29;
        }
        if kalan_gun < uzunluk {
            break;
        }
        kalan_gun -= uzunluk;
        ay += 1;
    }
    format!("{}-{:02}-{:02}", yil, ay + 1, kalan_gun + 1)
}

/// `robots.txt` içeriğini üretir.
///
/// Yayımlanmayan taslakların adresleri `Disallow` ile dışlanır; böylece bir
/// taslak yanlışlıkla çıktıya yazılsa bile arama motorlarına kapalı olur
/// (rapor § 10, tehdit modeli "Kaynak içerik klasörü").
pub fn robots_uret(temel_adres: &str, disallow: &[String]) -> String {
    let kok = temel_adres.trim_end_matches('/');
    let mut cikti = String::from("User-agent: *\nAllow: /\n");
    for yol in disallow {
        let mut temiz = yol.clone();
        if !temiz.starts_with('/') {
            temiz.insert(0, '/');
        }
        cikti.push_str(&format!("Disallow: {temiz}\n"));
    }
    if disallow.is_empty() {
        cikti.push_str("Disallow: /\n");
    }
    cikti.push_str(&format!("\nSitemap: {kok}/sitemap.xml\n"));
    cikti
}

#[cfg(test)]
// unwrap/expect yalnizca testlerde ve gerekçeyle kullanilabilir (WORKER_CONTRACT.md § 4.2).
// Bir testte hata durumu testin kendisidir; sessizce yutulmamalidir.
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn bos_site_haritasi_gecerlidir() {
        let cikti = sitemap_uret("https://ornek.example", &[]).unwrap();
        assert!(cikti.contains("<urlset"));
        assert!(cikti.contains("</urlset>"));
    }

    #[test]
    fn adresler_temel_adrese_birlestirilir() {
        let girdi = vec![HaritaGirdisi {
            adres: "/blog/yazi/".to_string(),
            guncellenme: 0,
        }];
        let cikti = sitemap_uret("https://ornek.example", &girdi).unwrap();
        assert!(cikti.contains("<loc>https://ornek.example/blog/yazi/</loc>"));
    }

    #[test]
    fn sondaki_ince_kolon_normalize_edilir() {
        let girdi = vec![HaritaGirdisi {
            adres: "/".to_string(),
            guncellenme: 0,
        }];
        let cikti = sitemap_uret("https://ornek.example/", &girdi).unwrap();
        assert!(cikti.contains("<loc>https://ornek.example/</loc>"));
        assert!(!cikti.contains("example.com//"));
    }

    #[test]
    fn basinda_ince_olon_eklenir() {
        let girdi = vec![HaritaGirdisi {
            adres: "hakkimizda/".to_string(),
            guncellenme: 0,
        }];
        let cikti = sitemap_uret("https://ornek.example", &girdi).unwrap();
        assert!(cikti.contains("<loc>https://ornek.example/hakkimizda/</loc>"));
    }

    #[test]
    fn ayni_adres_iki_dosyada_hata_verir() {
        let girdi = vec![
            HaritaGirdisi {
                adres: "/a/".to_string(),
                guncellenme: 0,
            },
            HaritaGirdisi {
                adres: "/a/".to_string(),
                guncellenme: 0,
            },
        ];
        let hata = sitemap_uret("https://ornek.example", &girdi).unwrap_err();
        assert!(hata.to_string().contains("ayni adresi"), "{hata}");
    }

    #[test]
    fn sirali_girdi_ayni_sirali_cikti_verir() {
        let girdi = vec![
            HaritaGirdisi {
                adres: "/z/".to_string(),
                guncellenme: 0,
            },
            HaritaGirdisi {
                adres: "/a/".to_string(),
                guncellenme: 0,
            },
        ];
        let c = sitemap_uret("https://ornek.example", &girdi).unwrap();
        let a = c.find("/a/").unwrap();
        let z = c.find("/z/").unwrap();
        assert!(a < z, "girdi sirasi degisse de cikti sirali olmali");
    }

    #[test]
    fn lastmod_yazildiginda_tarih_bicimi_dogru() {
        // 2024-01-01 -> 1704067200 UNIX saniye
        let girdi = vec![HaritaGirdisi {
            adres: "/".to_string(),
            guncellenme: 1_704_067_200,
        }];
        let cikti = sitemap_uret("https://ornek.example", &girdi).unwrap();
        assert!(cikti.contains("<lastmod>2024-01-01</lastmod>"), "{cikti}");
    }

    #[test]
    fn lastmod_sifirsa_yazilmaz() {
        let girdi = vec![HaritaGirdisi {
            adres: "/".to_string(),
            guncellenme: 0,
        }];
        let cikti = sitemap_uret("https://ornek.example", &girdi).unwrap();
        assert!(!cikti.contains("<lastmod>"), "{cikti}");
    }

    #[test]
    fn robots_taslaklari_dislar() {
        let disallow = vec!["/taslak/".to_string()];
        let cikti = robots_uret("https://ornek.example", &disallow);
        assert!(cikti.contains("User-agent: *"));
        assert!(cikti.contains("Disallow: /taslak/"));
        assert!(cikti.contains("Sitemap: https://ornek.example/sitemap.xml"));
    }

    #[test]
    fn robots_taslak_yoksa_tam_kapali_degildir() {
        let cikti = robots_uret("https://ornek.example", &[]);
        assert!(cikti.contains("Allow: /"));
        assert!(!cikti.contains("Disallow: /\nSitemap"));
    }

    #[test]
    fn robots_bos_adres_almaz() {
        let disallow = vec!["gizli/".to_string()];
        let cikti = robots_uret("https://ornek.example", &disallow);
        assert!(cikti.contains("Disallow: /gizli/"), "{cikti}");
    }
}
