//! Ön bilgi (front matter) ayrıştırıcısı — YAML'in belgelenmiş bir alt kümesi.
//!
//! Desteklenen sözdizimi yalnızca şudur:
//!
//! * `anahtar: deger` — tek satırlık ölçek değer.
//! * `anahtar: [a, b, c]` — tek satırlık akış dizisi.
//! * `anahtar:` + girintili `- oge` satırları — blok dizisi.
//! * `#` ile başlayan yorum satırları.
//!
//! Tanımsız alan adı **hata** üretir. Bu sert davranış bilinçlidir: sessizce
//! yutulan bir alan adı, üretimde aylar sonra fark edilen bozuk bir sayfaya
//! dönüşür (bkz. rapor § 05, "Ön bilgi başlığı alanları").

use serde::{Deserialize, Serialize};

use crate::error::Hata;

/// Bir içerik dosyasının başındaki `---` bloktan okunan üst veri.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct OnBilgi {
    /// Sayfa başlığı (`baslik`).
    pub baslik: Option<String>,
    /// Yayın tarihi, ham metin olarak saklanır (`tarih`).
    pub tarih: Option<String>,
    /// Etiketler (`etiketler`).
    pub etiketler: Vec<String>,
    /// Kısa özet (`ozet`).
    pub ozet: Option<String>,
    /// Yazar (`yazar`).
    pub yazar: Option<String>,
    /// Taslak sayfaları üretim dışında bırakılır (`taslak`).
    pub taslak: bool,
    /// Adres uzantısı (`slug`); verilmezse dosya adından türetilir.
    pub slug: Option<String>,
    /// Şablonda kullanılan sıralama anahtarı (`siralama`).
    pub siralama: Option<i64>,
}

impl OnBilgi {
    /// Ön bilgi alanlarının kabul edildiği liste.
    ///
    /// `pub` yerine `pub(crate)` değildir; belirtim dokümanında listelenebilsin
    /// diye `pub` bırakılmıştır.
    pub const ALANLAR: &'static [&'static str] = &[
        "baslik",
        "tarih",
        "etiketler",
        "ozet",
        "yazar",
        "taslak",
        "slug",
        "siralama",
    ];

    /// Ön bilgi metnini ayrıştırır.
    ///
    /// Blok yoksa (`---` ile başlamıyorsa) boş bir `OnBilgi` ve gövdenin tamamı
    /// döner. Blok varsa kapatılmalıdır.
    ///
    /// # Hatalar
    ///
    /// * Dosya `---` ile başlıyor ama kapanmıyorsa `Hata::OnBilgi`.
    /// * `anahtar: deger` biçimine uymayan satır varsa `Hata::OnBilgi`.
    /// * `ALANLAR` dışında bir alan adı varsa `Hata::OnBilgi`.
    /// * `taslak` ya da `siralama` yanlış tipteyse `Hata::OnBilgi`.
    pub fn ayikla(dosya_adi: &str, kaynak: &str) -> Result<(Self, String), Hata> {
        let satirlar: Vec<&str> = kaynak.split('\n').collect();
        let ilk = satirlar
            .first()
            .map(|s| s.trim_end_matches('\r'))
            .unwrap_or("");
        if ilk.trim_end() != "---" {
            return Ok((OnBilgi::default(), kaynak.to_string()));
        }

        let mut bitis: Option<usize> = None;
        for (indeks, satir) in satirlar.iter().enumerate().skip(1) {
            let satir = satir.trim_end_matches('\r');
            if satir.trim_end() == "---" {
                bitis = Some(indeks);
                break;
            }
        }
        let bitis = bitis.ok_or_else(|| {
            Hata::on_bilgi(
                dosya_adi,
                1,
                "on bilgi blogu `---` ile basladi ama kapatilmadi; dosyanin sonuna kadar acik kaldi",
            )
        })?;

        let govde = satirlar[bitis + 1..].join("\n");
        let mut on_bilgi = OnBilgi::default();
        let mut mevcut_anahtar: Option<String> = None;
        let mut liste_toplandi: Vec<String> = Vec::new();

        for (indeks, satir) in satirlar[1..bitis].iter().enumerate() {
            let numara = indeks + 2; // 1 tabanlı satır numarası, 1. satır `---`.
            let satir = satir.trim_end_matches('\r');

            if satir.trim().is_empty() || satir.trim_start().starts_with('#') {
                continue;
            }

            // Blok dizisi devam satırı: `  - oge`
            let girintili_dizi = satir.trim_start().starts_with("- ");
            if girintili_dizi {
                let oge = satir
                    .trim_start()
                    .trim_start_matches("- ")
                    .trim()
                    .to_string();
                match mevcut_anahtar.as_deref() {
                    Some("etiketler") => liste_toplandi.push(deger_coz(dosya_adi, numara, &oge)),
                    _ => {
                        return Err(Hata::on_bilgi(
                            dosya_adi,
                            numara,
                            "liste satiri yalnizca `etiketler:` anahtarindan sonra gelebilir",
                        ))
                    }
                }
                continue;
            }

            // Satır sonunda gelen bir blok dizisini kapat.
            if let Some(anahtar) = mevcut_anahtar.take() {
                if anahtar == "etiketler" {
                    on_bilgi.etiketler = std::mem::take(&mut liste_toplandi);
                }
            }

            let parcalar: Vec<&str> = satir.splitn(2, ':').collect();
            if parcalar.len() != 2 {
                return Err(Hata::on_bilgi(
                    dosya_adi,
                    numara,
                    format!("`anahtar: deger` bicimi bekleniyordu, satir: `{satir}`"),
                ));
            }
            let anahtar = parcalar[0].trim().to_string();
            let deger = parcalar[1].trim();

            if !OnBilgi::ALANLAR.contains(&anahtar.as_str()) {
                return Err(Hata::on_bilgi(
                    dosya_adi,
                    numara,
                    format!(
                        "tanimsiz alan `{}`; kabul edilenler: {}",
                        anahtar,
                        OnBilgi::ALANLAR.join(", ")
                    ),
                ));
            }

            match anahtar.as_str() {
                "baslik" | "tarih" | "ozet" | "yazar" => {
                    let cozulmus = deger_coz(dosya_adi, numara, deger);
                    if cozulmus.is_empty() {
                        return Err(Hata::on_bilgi(
                            dosya_adi,
                            numara,
                            format!("`{anahtar}` alani bos birakilamaz"),
                        ));
                    }
                    match anahtar.as_str() {
                        "baslik" => on_bilgi.baslik = Some(cozulmus),
                        "tarih" => on_bilgi.tarih = Some(cozulmus),
                        "ozet" => on_bilgi.ozet = Some(cozulmus),
                        _ => on_bilgi.yazar = Some(cozulmus),
                    }
                }
                "slug" => {
                    let cozulmus = deger_coz(dosya_adi, numara, deger);
                    on_bilgi.slug = Some(cozulmus);
                }
                "taslak" => {
                    on_bilgi.taslak = bool_coz(dosya_adi, numara, deger)?;
                }
                "siralama" => {
                    let cozulmus = deger_coz(dosya_adi, numara, deger);
                    on_bilgi.siralama = Some(i64_coz(dosya_adi, numara, &cozulmus)?);
                }
                "etiketler" => {
                    if deger.is_empty() {
                        mevcut_anahtar = Some(anahtar);
                    } else if deger.starts_with('[') {
                        let govde = deger.trim_start_matches('[').trim_end_matches(']');
                        let ogeler: Vec<String> = govde
                            .split(',')
                            .map(|o| deger_coz(dosya_adi, numara, o.trim()))
                            .filter(|o| !o.is_empty())
                            .collect();
                        on_bilgi.etiketler = ogeler;
                    } else {
                        on_bilgi.etiketler = vec![deger_coz(dosya_adi, numara, deger)];
                    }
                }
                _ => {
                    return Err(Hata::on_bilgi(
                        dosya_adi,
                        numara,
                        format!("islenemeyen alan `{anahtar}`"),
                    ))
                }
            }
        }

        if mevcut_anahtar.as_deref() == Some("etiketler") {
            on_bilgi.etiketler = liste_toplandi;
        }

        Ok((on_bilgi, govde))
    }
}

/// Tırnak içindeki veya çıplak bir ölçek değeri metne çevirir.
fn deger_coz(dosya: &str, satir: usize, ham: &str) -> String {
    let ham = ham.trim();
    if ham.len() >= 2 {
        let ilk = ham.chars().next();
        let son = ham.chars().last();
        if (ilk == Some('"') && son == Some('"')) || (ilk == Some('\'') && son == Some('\'')) {
            return ham[1..ham.len() - 1].to_string();
        }
    }
    // Satır sonu yorumu: `deger  # aciklama`
    if let Some(konum) = ham.find(" #") {
        return ham[..konum].trim().to_string();
    }
    let _ = (dosya, satir);
    ham.to_string()
}

/// `true` / `false` / `evet` / `hayir` / `1` / `0` değerlerini kabul eder.
fn bool_coz(dosya: &str, satir: usize, ham: &str) -> Result<bool, Hata> {
    match ham.to_ascii_lowercase().as_str() {
        "true" | "evet" | "1" | "yes" => Ok(true),
        "false" | "hayir" | "0" | "no" => Ok(false),
        _ => Err(Hata::on_bilgi(
            dosya,
            satir,
            format!("`taslak` alani `true` ya da `false` olmali, verilen: `{ham}`"),
        )),
    }
}

/// Ondalık ya da tam sayı metnini `i64` değerine çevirir.
fn i64_coz(dosya: &str, satir: usize, ham: &str) -> Result<i64, Hata> {
    ham.parse::<i64>().map_err(|_| {
        Hata::on_bilgi(
            dosya,
            satir,
            format!("tam sayi bekleniyordu, verilen: `{ham}`"),
        )
    })
}

#[cfg(test)]
// unwrap/expect yalnizca testlerde ve gerekçeyle kullanilabilir (WORKER_CONTRACT.md § 4.2).
// Bir testte hata durumu testin kendisidir; sessizce yutulmamalidir.
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn blok_yoksa_bos_on_bilgi_dondurur() {
        let (on_bilgi, govde) = OnBilgi::ayikla("a.md", "# Baslik\n\nGovde.").unwrap();
        assert_eq!(on_bilgi, OnBilgi::default());
        assert_eq!(govde, "# Baslik\n\nGovde.");
    }

    #[test]
    fn tam_on_bilgi_bloku_okur() {
        let kaynak = "---\nbaslik: Merhaba\ntarih: 2026-09-29\netiketler: [notlar, surec]\nozet: Ozet.\nyazar: web ekibi\ntaslak: false\n---\n\nGovde metni.\n";
        let (on_bilgi, govde) = OnBilgi::ayikla("a.md", kaynak).unwrap();
        assert_eq!(on_bilgi.baslik.as_deref(), Some("Merhaba"));
        assert_eq!(on_bilgi.tarih.as_deref(), Some("2026-09-29"));
        assert_eq!(on_bilgi.etiketler, vec!["notlar", "surec"]);
        assert_eq!(on_bilgi.ozet.as_deref(), Some("Ozet."));
        assert_eq!(on_bilgi.yazar.as_deref(), Some("web ekibi"));
        assert!(!on_bilgi.taslak);
        assert_eq!(govde.trim(), "Govde metni.");
    }

    #[test]
    fn taslak_true_islenir() {
        let kaynak = "---\nbaslik: T\ntaslak: true\n---\ngovde";
        let (on_bilgi, _) = OnBilgi::ayikla("a.md", kaynak).unwrap();
        assert!(on_bilgi.taslak);
    }

    #[test]
    fn blok_etiket_listesi_okur() {
        let kaynak = "---\nbaslik: T\netiketler:\n  - bir\n  - iki\n  - uc\n---\ngovde";
        let (on_bilgi, _) = OnBilgi::ayikla("a.md", kaynak).unwrap();
        assert_eq!(on_bilgi.etiketler, vec!["bir", "iki", "uc"]);
    }

    #[test]
    fn tirnakli_deger_tirnaksiz_okunur() {
        let kaynak = "---\nbaslik: \"Tirnak: icinde iki nokta\"\n---\ngovde";
        let (on_bilgi, _) = OnBilgi::ayikla("a.md", kaynak).unwrap();
        assert_eq!(on_bilgi.baslik.as_deref(), Some("Tirnak: icinde iki nokta"));
    }

    #[test]
    fn kapatmayan_blok_hata_verir() {
        let hata = OnBilgi::ayikla("a.md", "---\nbaslik: T\n\ngovde").unwrap_err();
        assert!(hata.to_string().contains("kapatilmadi"), "{hata}");
    }

    #[test]
    fn tanimsiz_alan_hata_verir() {
        let kaynak = "---\nbaslik: T\nyazim_hatasi: evet\n---\ngovde";
        let hata = OnBilgi::ayikla("a.md", kaynak).unwrap_err();
        assert!(hata.to_string().contains("tanimsiz alan"), "{hata}");
    }

    #[test]
    fn iki_noktasiz_satir_hata_verir() {
        let kaynak = "---\nbaslik T\n---\ngovde";
        let hata = OnBilgi::ayikla("a.md", kaynak).unwrap_err();
        assert!(hata.to_string().contains("anahtar: deger"), "{hata}");
    }

    #[test]
    fn bozuk_taslak_degeri_hata_verir() {
        let kaynak = "---\nbaslik: T\ntaslak: belki\n---\ngovde";
        let hata = OnBilgi::ayikla("a.md", kaynak).unwrap_err();
        assert!(hata.to_string().contains("true"), "{hata}");
    }

    #[test]
    fn siralama_tam_sayi_okur() {
        let kaynak = "---\nbaslik: T\nsiralama: -3\n---\ngovde";
        let (on_bilgi, _) = OnBilgi::ayikla("a.md", kaynak).unwrap();
        assert_eq!(on_bilgi.siralama, Some(-3));
    }

    #[test]
    fn siralama_ondalik_hata_verir() {
        let kaynak = "---\nbaslik: T\nsiralama: 1.5\n---\ngovde";
        let hata = OnBilgi::ayikla("a.md", kaynak).unwrap_err();
        assert!(hata.to_string().contains("tam sayi"), "{hata}");
    }

    #[test]
    fn bos_satirlar_ve_yorumlar_atlanir() {
        let kaynak = "---\n# yorum\n\nbaslik: T\n\n# baska yorum\n---\ngovde";
        let (on_bilgi, _) = OnBilgi::ayikla("a.md", kaynak).unwrap();
        assert_eq!(on_bilgi.baslik.as_deref(), Some("T"));
    }

    #[test]
    fn bos_etkiket_dizesi_izinlidir() {
        let kaynak = "---\nbaslik: T\netiketler: []\n---\ngovde";
        let (on_bilgi, _) = OnBilgi::ayikla("a.md", kaynak).unwrap();
        assert!(on_bilgi.etiketler.is_empty());
    }
}
