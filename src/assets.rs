//! Varlık boru hattı: `static/` klasöründen imzalı, kısaltılmış kopyalar üretir.
//!
//! Boru hattı üç adımdan oluşur (rapor § 05, "Ön yüz varlık boru hattı"):
//!
//! 1. **İçerik imzası** — FNV-1a 64-bit karma. Bu bir kriptografik karma
//!    **değildir**; amaç önbellek anahtarı üretmektir, güvenlik değil.
//! 2. **Bağlantı yeniden yazma** — metin varlıklardaki eski adresler, imzalı
//!    yeni adreslerle değiştirilir.
//! 3. **Kısaltma** — CSS/JS/HTML'de yalnızca güvenli kısaltmalar (satır sonu ve
//!    gereksiz boşluk) uygulanır. Anlamı değiştiren kısaltma yapılmaz.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::discover::guvenli_goreli;
use crate::error::Hata;

/// Varlık kökünün çıktı altındaki adı (tüm adresler `/static/...` ile başlar).
pub const VARLIK_ONEKI: &str = "static";

/// Bir varlığın kaynak adından üretilen adres ile eşleşmesi.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VarlikKaydi {
    /// Kaynak göreli yol (`css/sayfa.css`).
    pub kaynak_yol: String,
    /// Çıktı göreli yol (`static/css/sayfa.ab12cd34.css`).
    pub cikti_yol: String,
    /// Sitede kullanılacak adres (`/static/css/sayfa.ab12cd34.css`).
    pub adres: String,
    /// Kaynağın bayt cinsinden boyutu.
    pub boyut: u64,
}

/// Bir varlık kümesinin üretim sonucu.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VarlikRaporu {
    /// Ad → adres eşlemesi; şablonlardaki `varlik("...")` çağrıları bunu kullanır.
    pub harita: BTreeMap<String, String>,
    /// Üretilen dosyaların listesi.
    pub dosyalar: Vec<VarlikKaydi>,
}

impl VarlikRaporu {
    /// Bir kaynak göreli yolunun adresini verir; bilinmiyorsa girdi olduğu gibi döner.
    pub fn adres(&self, kaynak_yol: &str) -> String {
        self.harita
            .get(kaynak_yol)
            .cloned()
            .unwrap_or_else(|| format!("/{VARLIK_ONEKI}/{kaynak_yol}"))
    }
}

/// FNV-1a 64-bit karması.
///
/// Bu, kriptografik değil **önbellek anahtarı** karmasıdır; çakışma ihtimali
/// düşük ama sıfır değildir ve dosya adı güvenliği buna dayanmaz.
pub fn imza_al(veri: &[u8]) -> u64 {
    let mut karma: u64 = 0xcbf2_9ce4_8422_2325;
    for bayt in veri {
        karma ^= u64::from(*bayt);
        karma = karma.wrapping_mul(0x0000_0100_0000_01b3);
    }
    karma
}

/// Karma değerini dosya adına eklenecek sekiz onaltılık hâle getirir.
pub fn imza_metin(karma: u64) -> String {
    format!("{karma:016x}")
}

/// Varlık kökündeki tüm dosyaları imzalayıp kopyalar.
///
/// `cikti_kok` altındaki `static/` klasörüne yazılır. Dizin yapısı korunur.
pub fn isle(
    varlik_kok: &Path,
    cikti_kok: &Path,
    imzali: bool,
    kucult: bool,
) -> Result<VarlikRaporu, Hata> {
    let mut rapor = VarlikRaporu::default();
    if !varlik_kok.exists() {
        return Ok(rapor);
    }
    let mut kaynaklar: Vec<(String, PathBuf)> = Vec::new();
    topla(varlik_kok, varlik_kok, &mut kaynaklar, 0)?;
    kaynaklar.sort_by(|a, b| a.0.cmp(&b.0));

    for (goreli, tam) in kaynaklar {
        let veri = std::fs::read(&tam).map_err(|kaynak| Hata::disk(&tam, &kaynak))?;
        let hedef_ad = imzali_ad(&goreli, imza_al(&veri), imzali);
        let cikti_yol = format!("{VARLIK_ONEKI}/{hedef_ad}");
        let adres = format!("/{VARLIK_ONEKI}/{hedef_ad}");
        let boyut = veri.len() as u64;
        let kucultulmus_metin = kucult && metin_varlik_mi(&goreli);

        rapor.harita.insert(goreli.clone(), adres.clone());
        rapor.dosyalar.push(VarlikKaydi {
            kaynak_yol: goreli,
            cikti_yol: cikti_yol.clone(),
            adres,
            boyut,
        });

        // Bağlantı yeniden yazma yalnızca metin varlıklarda anlamlıdır.
        let hedef_yol = cikti_kok.join(&cikti_yol);
        if let Some(ust) = hedef_yol.parent() {
            std::fs::create_dir_all(ust).map_err(|kaynak| Hata::disk(ust, &kaynak))?;
        }
        let baytlar = if kucultulmus_metin {
            kucult_bytes(&veri)
        } else {
            veri
        };
        std::fs::write(&hedef_yol, baytlar).map_err(|kaynak| Hata::disk(&hedef_yol, &kaynak))?;
    }
    Ok(rapor)
}

/// İkinci geçiş: metin varlıklardaki eski `/static/...` adreslerini yeniden yazar.
///
/// Bu ayrı bir geçiştir çünkü bir varlığın içinde bir **önceki** varlığa
/// ait adres bulunabilir (örneğin `sayfa.css` içindeki `url(logo.png)`).
pub fn baglantilari_yeniden_yaz(rapor: &VarlikRaporu, cikti_kok: &Path) -> Result<usize, Hata> {
    let mut degisen = 0usize;
    for kayit in &rapor.dosyalar {
        if !metin_varlik_mi(&kayit.kaynak_yol) {
            continue;
        }
        let yol = cikti_kok.join(&kayit.cikti_yol);
        if !yol.exists() {
            continue;
        }
        let metin = std::fs::read_to_string(&yol).map_err(|kaynak| Hata::disk(&yol, &kaynak))?;
        let yeni = yeniden_yaz(&metin, &rapor.harita);
        if yeni != metin {
            std::fs::write(&yol, &yeni).map_err(|kaynak| Hata::disk(&yol, &kaynak))?;
            degisen += 1;
        }
    }
    Ok(degisen)
}

/// Metindeki `/static/<ad>` ve `static/<ad>` adreslerini imzalı hâlleriyle değiştirir.
pub fn yeniden_yaz(metin: &str, harita: &BTreeMap<String, String>) -> String {
    let mut sonuc = String::with_capacity(metin.len());
    let mut kalan = metin;
    // En uzun ad önce denenir ki `a.css` ile `a.min.css` karışmasın.
    let mut anahtarlar: Vec<&String> = harita.keys().collect();
    anahtarlar.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));

    'disarida: while let Some(konum) = kalan.find('/') {
        let mut uygun: Option<(&String, &String)> = None;
        for anahtar in &anahtarlar {
            let desen = format!("/{VARLIK_ONEKI}/{anahtar}");
            if kalan[konum..].starts_with(&desen) {
                uygun = Some((anahtar, harita.get(*anahtar).unwrap_or(anahtar)));
                break;
            }
        }
        match uygun {
            Some((_, hedef)) => {
                sonuc.push_str(&kalan[..konum]);
                sonuc.push_str(hedef);
                kalan = &kalan[konum..];
                let atlanacak = format!("/{VARLIK_ONEKI}/").len();
                let kalan_kaynak = &kalan[atlanacak..];
                let son = kalan_kaynak
                    .find(|c: char| {
                        !(c.is_alphanumeric() || c == '/' || c == '.' || c == '-' || c == '_')
                    })
                    .unwrap_or(kalan_kaynak.len());
                kalan = &kalan_kaynak[son..];
            }
            None => {
                let sonraki = konum
                    + kalan[konum..]
                        .chars()
                        .next()
                        .map(char::len_utf8)
                        .unwrap_or(1);
                sonuc.push_str(&kalan[..sonraki]);
                kalan = &kalan[sonraki..];
                continue 'disarida;
            }
        }
    }
    sonuc.push_str(kalan);
    sonuc
}

/// Bir varlık adına içerik imzasını ekler.
pub fn imzali_ad(goreli: &str, karma: u64, imzali: bool) -> String {
    if !imzali {
        return goreli.to_string();
    }
    let imza = imza_metin(karma);
    match goreli.rfind('.') {
        // Nokta, son segment dışında bir yolda varsa uzantı değildir.
        Some(konum) if konum > goreli.rfind('/').map(|s| s + 1).unwrap_or(0) => {
            format!("{}.{imza}{}", &goreli[..konum], &goreli[konum..])
        }
        _ => format!("{goreli}.{imza}"),
    }
}

/// Varlığın metin olup olmadığını uzantısından anlar.
pub fn metin_varlik_mi(goreli: &str) -> bool {
    let kucuk = goreli.to_ascii_lowercase();
    [
        ".css", ".js", ".html", ".htm", ".svg", ".txt", ".json", ".xml",
    ]
    .iter()
    .any(|sona| kucuk.ends_with(sona))
}

/// CSS, JS ve HTML için güvenli kısaltma: satır sonları ve gereksiz boşluk.
pub fn kucult_bytes(veri: &[u8]) -> Vec<u8> {
    match String::from_utf8(veri.to_vec()) {
        Ok(metin) => kucult(&metin).into_bytes(),
        Err(_) => veri.to_vec(),
    }
}

/// Metin kısaltma: yinelenen boşlukları ve boş satırları daraltır.
pub fn kucult(metin: &str) -> String {
    let mut satirlar: Vec<String> = Vec::new();
    let mut mevcut = String::new();
    let mut onceki_bosluk = false;
    for karakter in metin.chars() {
        match karakter {
            '\n' | '\r' => {
                satirlar.push(std::mem::take(&mut mevcut));
                onceki_bosluk = false;
            }
            ' ' | '\t' => {
                if !onceki_bosluk {
                    mevcut.push(' ');
                    onceki_bosluk = true;
                }
            }
            _ => {
                mevcut.push(karakter);
                onceki_bosluk = false;
            }
        }
    }
    satirlar.push(mevcut);
    // Boş satırlar ve satır sonu boşlukları atılır.
    let dolu: Vec<&str> = satirlar
        .iter()
        .map(|s| s.trim_end())
        .filter(|s| !s.is_empty())
        .collect();
    let mut sonuc = dolu.join("\n");
    if !sonuc.is_empty() {
        sonuc.push('\n');
    }
    sonuc
}

/// Varlık klasörünü özyinelemeli olarak gezer.
fn topla(
    kok: &Path,
    simdi: &Path,
    toplanan: &mut Vec<(String, PathBuf)>,
    derinlik: usize,
) -> Result<(), Hata> {
    if derinlik > 32 {
        return Err(Hata::Kullanim {
            ayrinti: format!("varlik klasoru cok derin: {}", simdi.display()),
        });
    }
    let girdiler = std::fs::read_dir(simdi).map_err(|kaynak| Hata::disk(simdi, &kaynak))?;
    for girdi in girdiler {
        let girdi = girdi.map_err(|kaynak| Hata::disk(simdi, &kaynak))?;
        let yol = girdi.path();
        let ad = girdi.file_name().to_string_lossy().to_string();
        if ad.starts_with('.') {
            continue;
        }
        let tip = girdi
            .file_type()
            .map_err(|kaynak| Hata::disk(&yol, &kaynak))?;
        if tip.is_dir() {
            topla(kok, &yol, toplanan, derinlik + 1)?;
        } else if tip.is_file() {
            let goreli = yol
                .strip_prefix(kok)
                .map(|p| p.to_string_lossy().to_string().replace('\\', "/"))
                .unwrap_or_default();
            if guvenli_goreli(&goreli) {
                toplanan.push((goreli, yol));
            }
        }
    }
    Ok(())
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
            let kok = std::env::temp_dir().join(format!(
                "siteturk-varlik-{}-{}",
                etiket,
                std::process::id()
            ));
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
    fn imza_karmasi_kararli() {
        // FNV-1a 64-bit resmî test vektörleri: boş girdinin karması
        // 0xcbf29ce484222325'dir (bkz. `## Atıflar`).
        assert_eq!(imza_al(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(imza_al(b"a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(imza_al(b"siteturk"), imza_al(b"siteturk"));
        assert_ne!(imza_al(b"a"), imza_al(b"b"));
        assert_eq!(imza_metin(imza_al(b"")).len(), 16);
    }

    #[test]
    fn imzali_ad_uzanti_ekler() {
        let ad = imzali_ad("css/sayfa.css", 0xabcd, true);
        assert_eq!(ad, "css/sayfa.000000000000abcd.css");
    }

    #[test]
    fn imzasiz_ad_degismez() {
        assert_eq!(imzali_ad("css/sayfa.css", 42, false), "css/sayfa.css");
    }

    #[test]
    fn uzantısız_ad_ismaya_imza_ekler() {
        assert_eq!(
            imzali_ad("veri/ozet", 0xff, true),
            "veri/ozet.00000000000000ff"
        );
    }

    #[test]
    fn metin_varlik_tespiti_yapar() {
        assert!(metin_varlik_mi("a/b.css"));
        assert!(metin_varlik_mi("a/b.SVG"));
        assert!(!metin_varlik_mi("a/b.png"));
    }

    #[test]
    fn kucult_bosluklari_daraltir() {
        assert_eq!(kucult("a   b\n\n\nc  "), "a b\nc\n");
    }

    #[test]
    fn kucult_bos_girdide_bos_verir() {
        assert_eq!(kucult(""), "");
    }

    #[test]
    fn baglanti_yeniden_yazma_calisir() {
        let mut harita = BTreeMap::new();
        harita.insert("logo.png".to_string(), "/static/logo.aa.png".to_string());
        let metin = "body { background: url('/static/logo.png'); }";
        let yeni = yeniden_yaz(metin, &harita);
        assert!(yeni.contains("/static/logo.aa.png"), "{yeni}");
        assert!(!yeni.contains("logo.png'"), "{yeni}");
    }

    #[test]
    fn bilinmeyen_varlik_adresi_oldugu_gibi_kalir() {
        let harita = BTreeMap::new();
        let metin = "url(/static/yok.png)";
        assert_eq!(yeniden_yaz(metin, &harita), metin);
    }

    #[test]
    fn varlik_raporu_klasor_isler() {
        let dizin = GeciciDizin::yeni("rapor").unwrap();
        dizin.yaz("static/sayfa.css", "body{color:red}").unwrap();
        dizin.yaz("static/img/logo.png", "PNG").unwrap();
        let cikti = dizin.yol.join("dist");
        let rapor = isle(&dizin.yol.join("static"), &cikti, true, true).unwrap();
        assert_eq!(rapor.dosyalar.len(), 2);
        let css_adres = rapor.adres("sayfa.css");
        assert!(css_adres.starts_with("/static/sayfa."), "{css_adres}");
        assert!(css_adres.ends_with(".css"), "{css_adres}");
        assert!(cikti.join(css_adres.trim_start_matches('/')).exists());
    }

    #[test]
    fn imzasiz_mod_dosya_adini_korur() {
        let dizin = GeciciDizin::yeni("imzasiz").unwrap();
        dizin.yaz("static/sayfa.css", "a{}").unwrap();
        let cikti = dizin.yol.join("dist");
        let rapor = isle(&dizin.yol.join("static"), &cikti, false, false).unwrap();
        assert_eq!(rapor.adres("sayfa.css"), "/static/sayfa.css");
    }

    #[test]
    fn varlik_klasoru_yoksa_bos_rapor_doner() {
        let cikti = std::env::temp_dir().join("siteturk-yok-boyle-bir-varlik");
        let rapor = isle(&cikti, &cikti, true, true).unwrap();
        assert!(rapor.dosyalar.is_empty());
        assert_eq!(rapor.adres("a.css"), "/static/a.css");
    }

    #[test]
    fn ayni_icerik_ayni_ad_uretir() {
        let bir = GeciciDizin::yeni("tekrarlanan1").unwrap();
        let iki = GeciciDizin::yeni("tekrarlanan2").unwrap();
        bir.yaz("static/a.css", "x").unwrap();
        iki.yaz("static/a.css", "x").unwrap();
        let b = isle(&bir.yol.join("static"), &bir.yol.join("d1"), true, false).unwrap();
        let c = isle(&iki.yol.join("static"), &iki.yol.join("d2"), true, false).unwrap();
        assert_eq!(b.adres("a.css"), c.adres("a.css"));
    }

    #[test]
    fn farkli_icerik_farkli_ad_uretir() {
        let dizin = GeciciDizin::yeni("farkli").unwrap();
        dizin.yaz("static/a.css", "x").unwrap();
        dizin.yaz("static/b.css", "y").unwrap();
        let rapor = isle(
            &dizin.yol.join("static"),
            &dizin.yol.join("dist"),
            true,
            false,
        )
        .unwrap();
        assert_ne!(rapor.adres("a.css"), rapor.adres("b.css"));
    }

    #[test]
    fn baglanti_yeniden_yazma_dosya_gunceller() {
        let dizin = GeciciDizin::yeni("yazma").unwrap();
        dizin.yaz("static/logo.png", "PNG").unwrap();
        dizin
            .yaz("static/sayfa.css", "body{background:url(/static/logo.png)}")
            .unwrap();
        let cikti = dizin.yol.join("dist");
        let rapor = isle(&dizin.yol.join("static"), &cikti, true, false).unwrap();
        let degisen = baglantilari_yeniden_yaz(&rapor, &cikti).unwrap();
        assert_eq!(degisen, 1);
        let yeni_adres = rapor.adres("logo.png");
        let css_yolu = cikti.join(rapor.dosyalar[1].cikti_yol.trim_start_matches('/'));
        let icerik = std::fs::read_to_string(&css_yolu).unwrap();
        assert!(icerik.contains(&yeni_adres), "{icerik}");
    }
}
