//! StatikUsta / Siteturk komut satırı arayüzü.
//!
//! Dört komut vardır:
//!
//! * `new <klasor>` — çalışan bir örnek site iskeleti üretir.
//! * `build` — siteyi `dist/` klasörüne üretir.
//! * `serve` — üretir, ardından yalnız yerelde dinleyen canlı yenileme
//!   sunucusunu açar ve dosya değişikliklerini yoklayarak yeniden üretir.
//! * `check` — yapılandırma, ön bilgi ve şablonları doğrular; **yazmaz**.

#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use clap::{Parser, Subcommand};

use siteturk::build::{self, UretimSecenekleri};
use siteturk::config::Yapilandirma;
use siteturk::discover;
use siteturk::error::Hata;
use siteturk::frontmatter::OnBilgi;
use siteturk::markdown;
use siteturk::serve::Sunucu;
use siteturk::template;

/// Bağımlılıksız Markdown statik site üreticisi.
#[derive(Debug, Parser)]
#[command(name = "siteturk", version, about, long_about = None)]
struct Kabuk {
    /// Üzerinde çalışılacak proje kökü (varsayılan: çalışma dizini).
    #[arg(long, short = 'p', value_name = "KLASOR", global = true)]
    proje: Option<PathBuf>,

    /// Alt komut.
    #[command(subcommand)]
    komut: Komut,
}

/// Alt komutlar.
#[derive(Debug, Subcommand)]
enum Komut {
    /// Çalışan bir örnek site iskeleti üretir.
    New {
        /// Oluşturulacak proje klasörü.
        #[arg(value_name = "KLASOR")]
        klasor: PathBuf,
    },
    /// Siteyi üretir.
    Build {
        /// Önbelleği yok say; her şeyi yeniden yaz.
        #[arg(long)]
        tam_uretim: bool,
        /// Üretim çıktısını ayrıntılı yazdır.
        #[arg(long, short = 'v')]
        ayrinti: bool,
    },
    /// Üretir ve yerel canlı yenileme sunucusunu açar.
    Serve {
        /// Dinlenecek port (yalnız 127.0.0.1).
        #[arg(long, value_name = "PORT", default_value_t = 8080)]
        port: u16,
        /// Dosya değişikliği yoklama aralığı (milisaniye).
        #[arg(long, value_name = "MS", default_value_t = 400)]
        yoklama_ms: u64,
    },
    /// Yapılandırma, ön bilgi ve şablonları doğrular; hiçbir şey yazmaz.
    Check,
}

fn main() -> ExitCode {
    let kabuk = Kabuk::parse();
    match calistir(kabuk) {
        Ok(()) => ExitCode::SUCCESS,
        Err(hata) => {
            eprintln!("hata: {hata}");
            ExitCode::FAILURE
        }
    }
}

/// Komutu çalıştırır.
fn calistir(kabuk: Kabuk) -> Result<(), Hata> {
    let proje = kabuk.proje.clone().unwrap_or_else(|| PathBuf::from("."));
    match &kabuk.komut {
        Komut::New { klasor } => yeni_site(klasor),
        Komut::Build {
            tam_uretim,
            ayrinti,
        } => {
            let yapilandirma = Yapilandirma::yukle(&proje)?;
            let uretim = build::uret(
                &proje,
                &yapilandirma,
                UretimSecenekleri {
                    tam_uretim: *tam_uretim,
                },
            )?;
            rapor_yaz(&uretim, *ayrinti);
            Ok(())
        }
        Komut::Serve { port, yoklama_ms } => sunucu_calistir(&proje, *port, *yoklama_ms),
        Komut::Check => kontrol(&proje),
    }
}

/// Üretim sonucunu konsola yazar.
fn rapor_yaz(uretim: &build::Uretim, ayrinti: bool) {
    let rapor = &uretim.rapor;
    println!(
        "uretildi: {} sayfa (yeniden {} · korunan {} · taslak {} · varlik {})",
        rapor.toplam_dosya, rapor.yeniden_uretilen, rapor.korunan, rapor.taslak, rapor.varlik
    );
    if rapor.yeniden_yazilan_varlik > 0 {
        println!(
            "varlik baglantilari yeniden yazildi: {}",
            rapor.yeniden_yazilan_varlik
        );
    }
    if rapor.silinen > 0 {
        println!("olu dosyalar temizlendi: {}", rapor.silinen);
    }
    if rapor.uyari > 0 {
        println!("markdown uyarilari ({}):", rapor.uyari);
        for uyari in &uretim.uyarilar {
            println!("  {uyari}");
        }
    }
    if ayrinti {
        println!("sure: {} ms", uretim.sure_ms);
        for adres in &uretim.adresler {
            println!("  sayfa: {adres}");
        }
        for kayit in &uretim.varliklar.dosyalar {
            println!("  varlik: {} ({} bayt)", kayit.adres, kayit.boyut);
        }
    }
}

/// `check` komutu: yazmadan doğrulama.
fn kontrol(proje: &Path) -> Result<(), Hata> {
    let yapilandirma = Yapilandirma::yukle(proje)?;
    println!("yapilandirma: okundu");
    println!("  kaynak: {}", yapilandirma.kaynak.display());
    println!("  cikti:  {}", yapilandirma.cikti.display());
    println!("  adres:  {}", yapilandirma.adres);

    let kaynak = proje.join(&yapilandirma.kaynak);
    if !kaynak.is_dir() {
        return Err(Hata::Kullanim {
            ayrinti: format!("icerik klasoru bulunamadi: {}", kaynak.display()),
        });
    }

    let sablon_yol = proje.join(&yapilandirma.sablon);
    let sablon_kaynak = std::fs::read_to_string(&sablon_yol).map_err(|hata| {
        if hata.kind() == std::io::ErrorKind::NotFound {
            Hata::Kullanim {
                ayrinti: format!("sablon dosyasi bulunamadi: {}", sablon_yol.display()),
            }
        } else {
            Hata::disk(&sablon_yol, &hata)
        }
    })?;
    template::Sablon::ayikla(
        &yapilandirma.sablon.display().to_string(),
        siteturk::bom_kaldir(&sablon_kaynak),
    )?;
    println!("sablon:    soz dizimi gecerli");

    let dosyalar = discover::markdown_dosyalarini_bul(&kaynak)?;
    let mut uyari_sayisi = 0usize;
    let mut adresler: Vec<String> = Vec::new();
    for dosya in &dosyalar {
        let icerik =
            std::fs::read_to_string(&dosya.kaynak).map_err(|e| Hata::disk(&dosya.kaynak, &e))?;
        let (on_bilgi, govde) = OnBilgi::ayikla(&dosya.goreli, siteturk::bom_kaldir(&icerik))?;
        let ayiklanmis = markdown::ayikla(&govde);
        uyari_sayisi += ayiklanmis.uyarilar.len();
        for uyari in &ayiklanmis.uyarilar {
            println!("  uyari: {}:{}: {}", dosya.goreli, uyari.satir, uyari.mesaj);
        }
        if on_bilgi.taslak && !yapilandirma.taslaklari_yayinla {
            continue;
        }
        adresler.push(match &on_bilgi.slug {
            Some(slug) => format!("/{}/", slug.trim_matches('/')),
            None => dosya.adres.clone(),
        });
    }
    adresler.sort();
    let tekrarlar: Vec<&String> = adresler
        .iter()
        .filter(|a| adresler.iter().filter(|b| *a == *b).count() > 1)
        .collect();
    if !tekrarlar.is_empty() {
        let liste: Vec<&str> = tekrarlar.iter().map(|a| a.as_str()).collect();
        return Err(Hata::Kullanim {
            ayrinti: format!(
                "birden fazla sayfa ayni adresi uretiyor: {}",
                liste.join(", ")
            ),
        });
    }
    println!(
        "icerik:    {} sayfa, {} uyari",
        dosyalar.len(),
        uyari_sayisi
    );
    println!("kontrol:   basarili — hata yok");
    Ok(())
}

/// Bağlantı yokken döngünün bekleyeceği kısa süre.
///
/// Dosya yoklaması `yoklama_ms` (varsayılan 400 ms) sıklığında çalışır; bağlantı
/// yoklaması ise bu kadar sık olsaydı her varlık isteği bir tam dosya taramasını
/// beklerdi. 20 ms, boştaki CPU yükünü ihmal edilebilir tutarken istek gecikmesini
/// de insan algısının altına indirir.
const BAGLANTI_BEKLEME: Duration = Duration::from_millis(20);

/// `serve` komutu: üret + dinle + değişiklikleri yokla.
fn sunucu_calistir(proje: &Path, port: u16, yoklama_ms: u64) -> Result<(), Hata> {
    let yapilandirma = Yapilandirma::yukle(proje)?;
    build::uret(proje, &yapilandirma, UretimSecenekleri::default())?;
    let cikti = proje.join(&yapilandirma.cikti);
    let mut sunucu = Sunucu::baslat(&cikti, port)?;
    println!("siteturk serve: http://{}", sunucu.yerel_adres());
    println!("yalnizca 127.0.0.1 dinleniyor · Ctrl+C ile dur");
    let aralik = Duration::from_millis(yoklama_ms.max(50));
    let mut onceki = izleri_tara(proje, &yapilandirma);
    let mut son_yoklama = Instant::now();

    loop {
        // Dosya değişikliği yoklaması: `notify` bağımlılığı yasaktır, bu yüzden
        // mtime imzaları düzenli aralıklarla karşılaştırılır. Bu tarama özyinelemeli
        // dosya gezintisi yaptığı için `aralik` sıklığında çalışır. Bağlantı
        // yoklaması ise çok daha sıktır: ikisi aynı kademede olsaydı her varlık
        // isteği bir tam yoklama turunu beklerdi ve sayfa yavaş açılırdı.
        if son_yoklama.elapsed() >= aralik {
            son_yoklama = Instant::now();
            let simdi = izleri_tara(proje, &yapilandirma);
            if simdi != onceki {
                onceki = simdi;
                match build::uret(proje, &yapilandirma, UretimSecenekleri::default()) {
                    Ok(uretim) => {
                        sunucu.uretim_artir();
                        println!(
                            "yeniden uretildi: {} sayfa (korunan {})",
                            uretim.rapor.yeniden_uretilen, uretim.rapor.korunan
                        );
                    }
                    Err(hata) => eprintln!("yeniden uretim hatasi: {hata}"),
                }
            }
        }
        // Bağlantı yoksa `tek_istek_isle` bloklamaz, `false` döner ve döngü kısa
        // bir süre bekler. Bağlantı varsa bekleme yapılmaz; böylece bir sayfanın
        // HTML ve varlık istekleri sırayla ve gecikmesiz işlenir.
        match sunucu.tek_istek_isle() {
            Ok(true) => {}
            Ok(false) => std::thread::sleep(BAGLANTI_BEKLEME),
            Err(hata) => {
                eprintln!("istek hatasi: {hata}");
                std::thread::sleep(BAGLANTI_BEKLEME);
            }
        }
    }
}

/// Kaynak ağacının mtime + boyut izlerini üretir.
fn izleri_tara(proje: &Path, yapilandirma: &Yapilandirma) -> Vec<(String, u128, u64)> {
    let mut izler = Vec::new();
    let mut kokler = vec![proje.join(&yapilandirma.kaynak)];
    if let Some(sablon) = proje
        .join(&yapilandirma.sablon)
        .parent()
        .map(|p| p.to_path_buf())
    {
        kokler.push(sablon);
    }
    for kok in kokler {
        if let Ok(dosyalar) = discover::markdown_dosyalarini_bul(&kok) {
            for dosya in dosyalar {
                izler.push((dosya.goreli, dosya.imza.mtime, dosya.imza.boyut));
            }
        }
    }
    izler.sort();
    izler
}

/// `new` komutu: çalışan bir örnek site iskeleti üretir.
fn yeni_site(klasor: &Path) -> Result<(), Hata> {
    if klasor.exists() {
        return Err(Hata::Kullanim {
            ayrinti: format!("klasor zaten var: {}", klasor.display()),
        });
    }
    std::fs::create_dir_all(klasor).map_err(|e| Hata::disk(klasor, &e))?;
    Yapilandirma::default().yaz(&klasor.join(Yapilandirma::DOSYA_ADI))?;
    ornek_icerigi(klasor)?;
    println!("olusturuldu: {}", klasor.display());
    println!("simdi calistir: cd {} && siteturk build", klasor.display());
    Ok(())
}

/// İskelet için örnek içerik, şablon ve varlıkları yazar.
fn ornek_icerigi(klasor: &Path) -> Result<(), Hata> {
    let sablonlar = klasor.join("sablonlar");
    std::fs::create_dir_all(&sablonlar).map_err(|e| Hata::disk(&sablonlar, &e))?;
    std::fs::write(
        sablonlar.join("sayfa.html"),
        include_str!("sablonlar/sayfa.html"),
    )
    .map_err(|e| Hata::disk(&sablonlar.join("sayfa.html"), &e))?;

    let icerik = klasor.join("icerik");
    std::fs::create_dir_all(icerik.join("blog")).map_err(|e| Hata::disk(&icerik, &e))?;
    std::fs::write(icerik.join("index.md"), include_str!("ornek/index.md"))
        .map_err(|e| Hata::disk(&icerik.join("index.md"), &e))?;
    std::fs::write(
        icerik.join("hakkimizda.md"),
        include_str!("ornek/hakkimizda.md"),
    )
    .map_err(|e| Hata::disk(&icerik.join("hakkimizda.md"), &e))?;
    std::fs::write(
        icerik.join("blog/neden.md"),
        include_str!("ornek/blog-neden.md"),
    )
    .map_err(|e| Hata::disk(&icerik.join("blog/neden.md"), &e))?;
    std::fs::write(
        icerik.join("blog/taslak-notlar.md"),
        include_str!("ornek/blog-taslak.md"),
    )
    .map_err(|e| Hata::disk(&icerik.join("blog/taslak-notlar.md"), &e))?;

    let statik = klasor.join("static");
    std::fs::create_dir_all(&statik).map_err(|e| Hata::disk(&statik, &e))?;
    std::fs::write(statik.join("sayfa.css"), include_str!("ornek/sayfa.css"))
        .map_err(|e| Hata::disk(&statik.join("sayfa.css"), &e))?;
    Ok(())
}

#[cfg(test)]
// unwrap/expect yalnizca testlerde ve gerekçeyle kullanilabilir (WORKER_CONTRACT.md § 4.2).
// Bir testte hata durumu testin kendisidir; sessizce yutulmamalidir.
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn kabuk_yorumlayici_yeni_site_ureter() {
        let kabuk = Kabuk::parse_from(["siteturk", "new", "hedef"]);
        match kabuk.komut {
            Komut::New { klasor } => assert_eq!(klasor, PathBuf::from("hedef")),
            _ => panic!("new komutu bekleniyordu"),
        }
    }

    #[test]
    fn kabuk_yorumlayici_build_tam_uretim_bayragini_okur() {
        let kabuk = Kabuk::parse_from(["siteturk", "build", "--tam-uretim"]);
        match kabuk.komut {
            Komut::Build { tam_uretim, .. } => assert!(tam_uretim),
            _ => panic!("build komutu bekleniyordu"),
        }
    }

    #[test]
    fn kabuk_yorumlayici_serve_port_varsayilani() {
        let kabuk = Kabuk::parse_from(["siteturk", "serve"]);
        match kabuk.komut {
            Komut::Serve { port, .. } => assert_eq!(port, 8080),
            _ => panic!("serve komutu bekleniyordu"),
        }
    }

    #[test]
    fn kabuk_yorumlayici_check_komutu() {
        let kabuk = Kabuk::parse_from(["siteturk", "check"]);
        assert!(matches!(kabuk.komut, Komut::Check));
    }

    #[test]
    fn proje_yolu_bayragi_yorumlanir() {
        let kabuk = Kabuk::parse_from(["siteturk", "-p", "site", "check"]);
        assert_eq!(kabuk.proje, Some(PathBuf::from("site")));
    }

    #[test]
    fn iz_taramasi_kaynak_agacini_ozetler() {
        let dizin = std::env::temp_dir().join(format!("siteturk-iz-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dizin);
        std::fs::create_dir_all(dizin.join("icerik")).unwrap();
        std::fs::write(dizin.join("icerik/a.md"), "a").unwrap();
        let izler = izleri_tara(&dizin, &Yapilandirma::default());
        assert_eq!(izler.len(), 1);
        assert_eq!(izler[0].0, "a.md");
        let _ = std::fs::remove_dir_all(&dizin);
    }

    #[test]
    fn kontrol_basarili_proje_doner() {
        let dizin = std::env::temp_dir().join(format!("siteturk-kontrol-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dizin);
        yeni_site(&dizin).unwrap();
        let sonuc = kontrol(&dizin);
        assert!(sonuc.is_ok(), "{:?}", sonuc.err());
        let _ = std::fs::remove_dir_all(&dizin);
    }
}
