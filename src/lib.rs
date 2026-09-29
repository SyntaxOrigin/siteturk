//! StatikUsta / Siteturk — bağımlılıksız statik site üreticisinin çekirdek kütüphanesi.
//!
//! Modüller tek yönlü bir hat izler ve hiçbiri geriye bağımlılık kurmaz:
//! `frontmatter` → `markdown` → `template` → `render` → `build`.
//! `serve` yalnızca `build`i çağırır; `main` yalnızca CLI kabuğudur.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![warn(clippy::unwrap_used, clippy::expect_used)]

pub mod assets;
pub mod build;
pub mod config;
pub mod discover;
pub mod error;
pub mod frontmatter;
pub mod markdown;
pub mod serve;
pub mod sitemap;
pub mod template;

pub use error::Hata;

/// Metnin başındaki UTF-8 BOM (`U+FEFF`) işaretlerini kaldırır, yoksa metni aynen döndürür.
///
/// Windows düzenleyicileri (Notepad dahil) ve PowerShell'in
/// `Set-Content -Encoding utf8` kipi dosyalara BOM ekler. `read_to_string`
/// bu baytı metnin içinde sakladığı için BOM taşıyan bir `.md` dosyasında
/// ön bilgi bloğu `---` ile başlamaz; blok **tanınmaz** ve dosyanın tamamı
/// paragraf metnine dönüşerek başlıksız, özetsiz bir sayfa üretir. Bu
/// davranış projenin "sessizce bozuk çıktı en kötü hatadır" kuralına aykırı
/// olduğu için BOM her metin okuma noktasında kaldırılır.
///
/// Baştaki **tüm** BOM'lar temizlenir, yalnızca biri değil: bir tane kalırsa
/// sorun sessizce sürmeye devam eder ve kullanıcı hatayı kaynağında arayamaz.
pub fn bom_kaldir(metin: &str) -> &str {
    metin.trim_start_matches('\u{feff}')
}

#[cfg(test)]
mod tests {
    use super::bom_kaldir;

    #[test]
    fn bom_ile_baslayan_metin_bom_suz_sonra_baslar() {
        assert_eq!(
            bom_kaldir("\u{feff}---\nbaslik: A\n---"),
            "---\nbaslik: A\n---"
        );
    }

    #[test]
    fn bomsuz_metin_degismez() {
        assert_eq!(bom_kaldir("---\nbaslik: A\n---"), "---\nbaslik: A\n---");
    }

    #[test]
    fn ardisik_bomlarin_tumu_kaldirilir() {
        assert_eq!(bom_kaldir("\u{feff}\u{feff}\u{feff}{}"), "{}");
    }

    #[test]
    fn yalnizca_bastaki_bom_kaldirilir_icerideki_korunur() {
        assert_eq!(bom_kaldir("a\u{feff}b"), "a\u{feff}b");
    }

    #[test]
    fn bomlu_bos_metin_bos_kalir() {
        assert_eq!(bom_kaldir("\u{feff}"), "");
    }
}
