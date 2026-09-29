//! Dahili canlı yenileme sunucusu — `std::net::TcpListener` ile, yalnız yerel.
//!
//! # Güvenlik modeli
//!
//! * Sunucu **yalnızca** `127.0.0.1` adresine bağlanır. Başka arayüz seçeneği
//!   yoktur ve olamaz; bu, rapor § 10'daki "ağ yüzeyi" tehditini kapatır.
//! * İstek yolundaki `..`, mutlak yol, NUL baytı ve yüzde kaçışları reddedilir.
//!   Dosya, sunucu kökünün **altında** kaldığı doğrulanmadan sunulmaz.
//!   `canonicalize` ile sembolik bağlantı ve `..` kaçışı da engellenir.
//! * Yalnız `GET` ve `HEAD` desteklenir; yazma yönü yoktur.
//!
//! # Canlı yenileme
//!
//! Tarayıcıya servis edilen HTML'e küçük bir yoklama betiği enjekte edilir.
//! Betik `500 ms` aralıkla `/__siteturk/durum` adresine gider; sunucudaki üretim
//! sayacı değiştiyse `location.reload()` çağırır. `tokio`/`hyper` yasak
//! olduğundan sunucu **tek iş parçacıklıdır** ve bağlantı başına tek istek
//! işlenir. `accept` engellemeyen kipte çalışır: bağlantı yoksa döngü dosya
//! yoklamasına devam eder, aksi hâlde tarayıcı kapalıyken sunucu körleşir ve
//! dosya değişiklikleri hiç fark edilmezdi.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, SocketAddrV4, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::error::Hata;

/// Sunucunun tek seferde okuyacağı en fazla isket başlığı boyutu.
const EN_FAZLA_ISTEK_BOYUTU: usize = 8 * 1024;

/// Sunucunun tek seferde göndereceği en fazla gövde boyutu (bayt).
const EN_FAZLA_GOVDE_BOYUTU: usize = 16 * 1024 * 1024;

/// Kabul edilen bir bağlantıdan istek başlıklarının bekleneceği azami süre.
///
/// Bağlantı kabul edildikten sonra akış engellemeye alınır; bu süre aşılmazsa
/// açık bırakılmış bir bağlantı döngüyü kilitler. Yerel geliştirme
/// sunucusu için iki saniye fazlasıyla yeterlidir.
const ISTEK_ZAMAN_ASIMI: Duration = Duration::from_secs(2);

/// Canlı yenileme yoklamasının kullanıldığı özel adres.
pub const DURUM_YOLU: &str = "/__siteturk/durum";

/// Ayrıştırılmış bir HTTP isteğinin yalnız ihtiyaç duyulan kısımları.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Istek {
    /// HTTP yöntemi (`GET`, `HEAD`, ...).
    pub yontem: String,
    /// Yüzde kaçışları çözülmüş, sorgu ayrılmış yol.
    pub yol: String,
    /// Sorgu dizesi (yoksa boş).
    pub sorgu: String,
    /// HTTP sürüm dizesi.
    pub surum: String,
}

/// Sunucunun bir isteğe verdiği tam yanıt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Yanit {
    /// Durum kodu.
    pub kod: u16,
    /// İçerik tipi başlığı.
    pub icerik_tipi: &'static str,
    /// Gövde baytları.
    pub govde: Vec<u8>,
    /// `Content-Length` başlığı yazılsın mı (HEAD isteklerinde hayır).
    pub uzunluk_yaz: bool,
}

impl Yanit {
    /// Metin gövdeli bir yanıt üretir.
    pub fn metin(kod: u16, icerik_tipi: &'static str, govde: &str) -> Self {
        Yanit {
            kod,
            icerik_tipi,
            govde: govde.as_bytes().to_vec(),
            uzunluk_yaz: true,
        }
    }

    /// HTTP/1.1 yanıtını bayt dizisine çevirir.
    pub fn bayta(self, basliklar: &[(&str, String)]) -> Vec<u8> {
        let neden = durum_nedeni(self.kod);
        let mut cikti = format!("HTTP/1.1 {} {}\r\n", self.kod, neden);
        cikti.push_str(&format!("Content-Type: {}\r\n", self.icerik_tipi));
        cikti.push_str("Cache-Control: no-store\r\n");
        cikti.push_str("X-Content-Type-Options: nosniff\r\n");
        for (ad, deger) in basliklar {
            cikti.push_str(&format!("{ad}: {deger}\r\n"));
        }
        if self.uzunluk_yaz {
            cikti.push_str(&format!("Content-Length: {}\r\n", self.govde.len()));
        }
        cikti.push_str("Connection: close\r\n\r\n");
        let mut bayt = cikti.into_bytes();
        bayt.extend_from_slice(&self.govde);
        bayt
    }
}

/// HTTP durum kodunun standart neden ifadesi.
pub fn durum_nedeni(kod: u16) -> &'static str {
    match kod {
        200 => "OK",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        500 => "Internal Server Error",
        _ => "Unknown",
    }
}

/// Bir uzantıya karşılık gelen MIME tipi.
pub fn mime_tipi(uzanti: &str) -> &'static str {
    match uzanti.to_ascii_lowercase().as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "xml" => "application/xml; charset=utf-8",
        "txt" | "md" => "text/plain; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "woff2" => "font/woff2",
        "pdf" => "application/pdf",
        _ => "application/octet-stream",
    }
}

/// Ham istek metnini ayrıştırır.
///
/// Sözdizimi hatası `None` döner; sunucu bunu `400` yanıtına çevirir.
pub fn istek_ayikla(ham: &str) -> Option<Istek> {
    let ilk_satir = ham.lines().next()?;
    let parcalar: Vec<&str> = ilk_satir.split_whitespace().collect();
    if parcalar.len() < 3 {
        return None;
    }
    let yontem = parcalar[0].to_string();
    let hedef = parcalar[1];
    let surum = parcalar[2].to_string();
    if !surum.starts_with("HTTP/") {
        return None;
    }
    let (yol_ham, sorgu) = match hedef.split_once('?') {
        Some((y, s)) => (y, s.to_string()),
        None => (hedef, String::new()),
    };
    let yol = yol_kodu_coz(yol_ham)?;
    Some(Istek {
        yontem,
        yol,
        sorgu,
        surum,
    })
}

/// Yüzde kaçışlarını çözer; geçersiz kaçış veya NUL baytı `None` verir.
pub fn yol_kodu_coz(girdi: &str) -> Option<String> {
    let baytlar = girdi.as_bytes();
    let mut cikti: Vec<u8> = Vec::with_capacity(baytlar.len());
    let mut i = 0usize;
    while i < baytlar.len() {
        if baytlar[i] == b'%' {
            if i + 2 >= baytlar.len() {
                return None;
            }
            let yuksek = (baytlar[i + 1] as char).to_digit(16)?;
            let alcak = (baytlar[i + 2] as char).to_digit(16)?;
            let deger = (yuksek * 16 + alcak) as u8;
            if deger == 0 {
                return None;
            }
            cikti.push(deger);
            i += 3;
        } else {
            cikti.push(baytlar[i]);
            i += 1;
        }
    }
    String::from_utf8(cikti).ok()
}

/// Bir istek yolunun sunucu kökünde güvenli bir dosyaya karşılık olup olmadığını
/// doğrular ve mutlak yolu döner.
///
/// # Güvenlik
///
/// Reddedilen durumlar: mutlak sürücü gösterimi (`C:`), ters eğik çizgi, `..`
/// ve `.` segmentleri, NUL baytı. `..` denetimi **yol metninde** yapılır (klasör
/// çifti kuralına güvenilmez). Kökün kendisi (`/`) geçerlidir; `adresi_dosyaya`
/// onu `index.html` dosyasına çevirir ve `is_file` denetimi eler.
pub fn guvenli_yolcoz(kok: &Path, istek_yolu: &str) -> Option<PathBuf> {
    if istek_yolu.is_empty() {
        return None;
    }
    if istek_yolu.contains('\0') {
        return None;
    }
    // Mutlak yol kabul edilir ancak daima sunucu kökünün altına sabitlenir.
    let yol = istek_yolu.trim_start_matches('/');
    if yol.contains('\\') {
        // Ters eğik çizgi Windows'ta ayırıcıdır; karıştırmayı engelle.
        return None;
    }
    for segment in yol.split('/') {
        if segment == ".." || segment == "." {
            return None;
        }
        if segment.len() >= 2 && segment.as_bytes()[1] == b':' {
            return None;
        }
    }
    let mut aday = kok.to_path_buf();
    for segment in yol.split('/') {
        if segment.is_empty() {
            continue;
        }
        aday.push(segment);
    }
    Some(aday)
}

/// Bir adres yolunu dosya sistemindeki karşılığına çevirir (dizin → `index.html`).
pub fn adresi_dosyaya(kok: &Path, istek_yolu: &str) -> Option<PathBuf> {
    let mut yol = guvenli_yolcoz(kok, istek_yolu)?;
    if istek_yolu.ends_with('/') || yol.is_dir() {
        yol.push("index.html");
    }
    Some(yol)
}
/// Dosyanın sunucu kökünün altında kaldığını doğrular (sembolik bağlantı denetimi).
pub fn kok_altinda(kok: &Path, aday: &Path) -> bool {
    let kok_mutlak = match kok.canonicalize() {
        Ok(y) => y,
        Err(_) => return false,
    };
    let aday_mutlak = match aday.canonicalize() {
        Ok(y) => y,
        Err(_) => return false,
    };
    aday_mutlak.starts_with(&kok_mutlak)
}

/// Canlı yenileme betiğinin HTML'e enjekte edildiği hâli.
pub const YENILEME_BETIGI: &str = r#"<script>
(function(){
  var k=0;
  function yokla(){
    fetch('/__siteturk/durum?k='+k,{cache:'no-store'}).then(function(r){return r.text();})
      .then(function(t){ if(t!==String(k)){ location.reload(); } k=t; setTimeout(yokla,500); })
      .catch(function(){ setTimeout(yokla,1000); });
  }
  yokla();
})();
</script>"#;

/// HTML gövdesine canlı yenileme betiğini enjekte eder.
pub fn betigi_ekle(html: &str) -> String {
    match html.rfind("</body>") {
        Some(konum) => {
            let mut sonuc = String::with_capacity(html.len() + YENILEME_BETIGI.len() + 16);
            sonuc.push_str(&html[..konum]);
            sonuc.push_str(YENILEME_BETIGI);
            sonuc.push('\n');
            sonuc.push_str(&html[konum..]);
            sonuc
        }
        None => {
            let mut sonuc = html.to_string();
            sonuc.push_str(YENILEME_BETIGI);
            sonuc.push('\n');
            sonuc
        }
    }
}

/// Çalışan canlı yenileme sunucusu.
pub struct Sunucu {
    dinleyici: TcpListener,
    kok: PathBuf,
    uretim_sayaci: u64,
}

impl Sunucu {
    /// Yalnız geri döngü arayüzüne bağlanan sunucu oluşturur.
    ///
    /// # Hatalar
    ///
    /// Port bağlanamazsa `Hata::Sunucu` döner.
    pub fn baslat(kok: &Path, port: u16) -> Result<Self, Hata> {
        let adres = SocketAddrV4::new(Ipv4Addr::LOCALHOST, port);
        let dinleyici = TcpListener::bind(adres).map_err(|hata| Hata::Sunucu {
            ayrinti: format!("127.0.0.1:{port} adresine baglanilamadi: {hata}"),
        })?;
        // `accept` bloklayan bir çağrıdır. Engellenirse dosya yoklama döngüsü
        // hiç dönmez ve canlı yenileme hiç tetiklenmez: tarayıcı kapalıyken
        // sunucu körleşir, dosya değişse bile `dist/` güncellenmez. Bu yüzden
        // dinleyici engellemeyen moda alınır; bağlantı yoksa `accept` anında
        // `WouldBlock` döner ve çağıran taraf yoklamaya devam eder.
        dinleyici
            .set_nonblocking(true)
            .map_err(|hata| Hata::Sunucu {
                ayrinti: format!("dinleyici kipi degistirilemedi: {hata}"),
            })?;
        Ok(Sunucu {
            dinleyici,
            kok: kok.to_path_buf(),
            uretim_sayaci: 0,
        })
    }

    /// Dinlenen adresi döner.
    pub fn yerel_adres(&self) -> String {
        self.dinleyici
            .local_addr()
            .map(|a| a.to_string())
            .unwrap_or_else(|_| "127.0.0.1:0".to_string())
    }

    /// Bağlantı kabul eder ve **tek** isteği işler.
    ///
    /// Sunucu tek iş parçacıklıdır; bu, "aynı anda iki bağlantı" senaryosunda
    /// ara durumun görünmemesini garanti eder.
    ///
    /// Gelen bağlantı yoksa bloklamaz, `Ok(false)` döner. Çağıran taraf bu
    /// sayede dosya yoklamasını sürdürebilir.
    pub fn tek_istek_isle(&mut self) -> Result<bool, Hata> {
        let (akis, _) = match self.dinleyici.accept() {
            Ok(baglanti) => baglanti,
            Err(hata) if hata.kind() == std::io::ErrorKind::WouldBlock => return Ok(false),
            Err(hata) => {
                return Err(Hata::Sunucu {
                    ayrinti: format!("baglanti kabul edilemedi: {hata}"),
                })
            }
        };
        // Kabul edilen akış da engellemeyen kipi miras alabilir. İstek
        // başlıkları gelene kadar beklemesi için akış yeniden engellemeye
        // alınır ve bekleme süresi sınırlanır: sınır olmazsa açık bırakılmış
        // bir bağlantı tek iş parçacıklı döngüyü süresiz kilitler.
        akis.set_nonblocking(false).map_err(|hata| Hata::Sunucu {
            ayrinti: format!("baglanti kipi duzeltilemedi: {hata}"),
        })?;
        akis.set_read_timeout(Some(ISTEK_ZAMAN_ASIMI))
            .map_err(|hata| Hata::Sunucu {
                ayrinti: format!("baglanti zaman asimi ayarlanamadi: {hata}"),
            })?;
        self.istegi_isle(akis)?;
        Ok(true)
    }

    /// Bir bağlantı üzerindeki isteği okur ve yanıtı yazar.
    fn istegi_isle(&mut self, akis: TcpStream) -> Result<(), Hata> {
        let mut okuyucu = BufReader::new(akis.try_clone().map_err(|hata| Hata::Sunucu {
            ayrinti: hata.to_string(),
        })?);
        let mut ham = String::new();
        // Başlık satırları boş satıra kadar okunur.
        loop {
            let onceki = ham.len();
            if okuyucu.read_line(&mut ham).is_err() {
                break;
            }
            if ham.len() == onceki {
                break;
            }
            if ham.len() > EN_FAZLA_ISTEK_BOYUTU {
                break;
            }
            let yeni_kisim = &ham[onceki..];
            if yeni_kisim == "\r\n" || yeni_kisim == "\n" {
                break;
            }
        }
        let istek = istek_ayikla(&ham);
        let (istek, gecersiz) = match istek {
            Some(i) => (i, false),
            None => (
                Istek {
                    yontem: "GET".to_string(),
                    yol: "/".to_string(),
                    sorgu: String::new(),
                    surum: "HTTP/1.1".to_string(),
                },
                true,
            ),
        };
        let yanit = if gecersiz {
            Yanit::metin(
                400,
                "text/plain; charset=utf-8",
                "400 — bozuk istek satiri\n",
            )
        } else {
            self.yanit_uret(&istek)
        };
        let govde_dahil = !istek.yontem.eq_ignore_ascii_case("HEAD");
        let baytlar = yanit.bayta(&[]);
        let mut yazilacak = baytlar;
        if !govde_dahil {
            // HEAD yanıtında gövde gönderilmez; başlıklar korunur.
            let ayirac = yazilacak
                .windows(4)
                .position(|p| p == b"\r\n\r\n")
                .map(|k| k + 4)
                .unwrap_or(yazilacak.len());
            yazilacak.truncate(ayirac);
        }
        let _ = akis.set_write_timeout(Some(Duration::from_secs(10)));
        let mut yaz = akis;
        let _ = yaz.write_all(&yazilacak);
        let _ = yaz.flush();
        Ok(())
    }

    /// İstek için dosya sisteminden yanıt üretir.
    ///
    /// Bu, sunucunun saf çekirdeğidir: yalnızca girdi alır, diske **okur** ve
    /// yanıt üretir. Soket işlemi içermediği için entegrasyon testlerinde
    /// doğrudan sınanabilir.
    ///
    /// # Durum kodları
    ///
    /// * `405` — yalnız `GET` ve `HEAD`.
    /// * `400` — yol geçişi (`..`), ters eğik çizgi ya da sürücü gösterimi içeriyor.
    /// * `404` — dosya yok.
    /// * `403` — dosya var ama sunucu kökünün dışına çözülüyor (sembolik bağlantı).
    /// * `200` — dosya sunuldu.
    pub fn yanit_uret(&mut self, istek: &Istek) -> Yanit {
        if istek.yontem != "GET" && istek.yontem != "HEAD" {
            return Yanit::metin(
                405,
                "text/plain; charset=utf-8",
                "405 — yalnizca GET ve HEAD desteklenir\n",
            );
        }
        if istek.yol == DURUM_YOLU {
            let sayi = self.uretim_sayaci.to_string();
            return Yanit::metin(200, "text/plain; charset=utf-8", &sayi);
        }
        match self.dosya_oku(&istek.yol) {
            Ok(yanit) => yanit,
            Err(kod) => Yanit::metin(
                kod,
                "text/plain; charset=utf-8",
                &format!("{kod} — istenen kaynak bulunamadi veya erisim reddedildi\n"),
            ),
        }
    }

    /// Sunucu kökünden bir dosya okur ve yanıta çevirir.
    fn dosya_oku(&mut self, istek_yolu: &str) -> Result<Yanit, u16> {
        let aday = adresi_dosyaya(&self.kok, istek_yolu).ok_or(400u16)?;
        // Sıra önemlidir: önce varlık, sonra kök içinde olma denetimi. Aksi hâlde
        // var olmayan bir yol "yok" değil "yasak" görünür ve hata kodu yanıltır.
        if !aday.is_file() {
            return Err(404);
        }
        if !kok_altinda(&self.kok, &aday) {
            return Err(403);
        }
        let mut dosya = std::fs::File::open(&aday).map_err(|_| 404u16)?;
        let mut baytlar = Vec::new();
        Read::by_ref(&mut dosya)
            .take(EN_FAZLA_GOVDE_BOYUTU as u64)
            .read_to_end(&mut baytlar)
            .map_err(|_| 500u16)?;
        let uzanti = aday
            .extension()
            .map(|e| e.to_string_lossy().to_string())
            .unwrap_or_default();
        let mut icerik_tipi = mime_tipi(&uzanti);
        let mut govde = baytlar;
        if icerik_tipi.starts_with("text/html") {
            let metin = String::from_utf8_lossy(&govde).to_string();
            govde = betigi_ekle(&metin).into_bytes();
            icerik_tipi = "text/html; charset=utf-8";
        }
        Ok(Yanit {
            kod: 200,
            icerik_tipi,
            govde,
            uzunluk_yaz: true,
        })
    }

    /// Üretim sayacını artırır; tarayıcı yoklaması bunu görür.
    pub fn uretim_artir(&mut self) -> u64 {
        self.uretim_sayaci += 1;
        self.uretim_sayaci
    }

    /// Üretim sayacının değerini döner.
    pub fn uretim_sayaci(&self) -> u64 {
        self.uretim_sayaci
    }

    /// Sunucu kökünü döner.
    pub fn kok(&self) -> &Path {
        &self.kok
    }
}

#[cfg(test)]
// unwrap/expect yalnizca testlerde ve gerekçeyle kullanilabilir (WORKER_CONTRACT.md § 4.2).
// Bir testte hata durumu testin kendisidir; sessizce yutulmamalidir.
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    struct GeciciDizin {
        yol: PathBuf,
    }

    impl GeciciDizin {
        fn yeni(etiket: &str) -> std::io::Result<Self> {
            let yol = std::env::temp_dir().join(format!(
                "siteturk-sunucu-{}-{}",
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
    }

    impl Drop for GeciciDizin {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.yol);
        }
    }

    #[test]
    fn istek_ayikla_get_okur() {
        let istek = istek_ayikla("GET /a/b.html HTTP/1.1\r\nHost: x\r\n\r\n").unwrap();
        assert_eq!(istek.yontem, "GET");
        assert_eq!(istek.yol, "/a/b.html");
        assert_eq!(istek.sorgu, "");
        assert_eq!(istek.surum, "HTTP/1.1");
    }

    #[test]
    fn istek_ayikla_sorguyu_ayirir() {
        let istek = istek_ayikla("GET /durum?k=3 HTTP/1.1\r\n\r\n").unwrap();
        assert_eq!(istek.yol, "/durum");
        assert_eq!(istek.sorgu, "k=3");
    }

    #[test]
    fn istek_ayikla_yuzde_kacisi_cozer() {
        let istek = istek_ayikla("GET /hakk%C4%B1m%C4%B1zda/ HTTP/1.1\r\n\r\n").unwrap();
        assert_eq!(istek.yol, "/hakkımızda/");
    }

    #[test]
    fn istek_ayikla_bozuk_satiri_reddeder() {
        assert!(istek_ayikla("GARBAGE\r\n\r\n").is_none());
        assert!(istek_ayikla("GET /a\r\n\r\n").is_none());
        assert!(istek_ayikla("").is_none());
    }

    #[test]
    fn yol_kodu_coz_gecersiz_kacisi_reddeder() {
        assert!(yol_kodu_coz("/a%zz").is_none());
        assert!(yol_kodu_coz("/a%2").is_none());
        assert!(yol_kodu_coz("/a%00b").is_none());
    }

    #[test]
    fn ustdizin_kacisi_reddedilir() {
        let kok = Path::new("C:/kok");
        assert!(guvenli_yolcoz(kok, "/../../gizli").is_none());
        assert!(guvenli_yolcoz(kok, "/blog/../../gizli").is_none());
        assert!(guvenli_yolcoz(kok, "/a/./b").is_none());
    }

    #[test]
    fn kok_yolu_index_html_cozulur() {
        let kok = Path::new("C:/kok");
        assert_eq!(guvenli_yolcoz(kok, "/").unwrap(), Path::new("C:/kok"));
        let sonuc = adresi_dosyaya(kok, "/").unwrap();
        assert_eq!(sonuc, Path::new("C:/kok").join("index.html"));
    }

    #[test]
    fn bos_yol_reddedilir() {
        assert!(guvenli_yolcoz(Path::new("C:/kok"), "").is_none());
    }

    #[test]
    fn nul_bayti_reddedilir() {
        assert!(guvenli_yolcoz(Path::new("C:/kok"), "/a\0b").is_none());
    }

    #[test]
    fn surucu_gosterimi_reddedilir() {
        assert!(guvenli_yolcoz(Path::new("C:/kok"), "/C:/Windows").is_none());
    }

    #[test]
    fn ters_eğik_cizgi_reddedilir() {
        assert!(guvenli_yolcoz(Path::new("C:/kok"), "/a\\..\\..\\b").is_none());
    }

    #[test]
    fn normal_yol_cozulur() {
        let kok = Path::new("C:/kok");
        let sonuc = guvenli_yolcoz(kok, "/blog/yazi.html").unwrap();
        assert_eq!(sonuc, Path::new("C:/kok").join("blog").join("yazi.html"));
    }

    #[test]
    fn dizin_yolu_index_html_ekler() {
        let kok = Path::new("C:/kok");
        let sonuc = adresi_dosyaya(kok, "/blog/").unwrap();
        assert!(sonuc.ends_with("index.html"));
    }

    #[test]
    fn mime_tipleri_dogru() {
        assert_eq!(mime_tipi("html"), "text/html; charset=utf-8");
        assert_eq!(mime_tipi("CSS"), "text/css; charset=utf-8");
        assert_eq!(mime_tipi("png"), "image/png");
        assert_eq!(mime_tipi("bilinmeyen"), "application/octet-stream");
    }

    #[test]
    fn durum_nedenleri_bilinen() {
        assert_eq!(durum_nedeni(200), "OK");
        assert_eq!(durum_nedeni(403), "Forbidden");
        assert_eq!(durum_nedeni(404), "Not Found");
        assert_eq!(durum_nedeni(499), "Unknown");
    }

    #[test]
    fn yanit_bayta_cevrisi_baslik_yazar() {
        let baytlar = Yanit::metin(200, "text/html; charset=utf-8", "x").bayta(&[]);
        let metin = String::from_utf8_lossy(&baytlar);
        assert!(metin.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(metin.contains("Content-Length: 1\r\n"));
        assert!(metin.ends_with("x"));
    }

    #[test]
    fn yenileme_betigi_body_icine_enjekte_edilir() {
        let sonuc = betigi_ekle("<html><body><p>a</p></body></html>");
        assert!(sonuc.contains("__siteturk/durum"));
        assert!(sonuc.find("__siteturk").unwrap() < sonuc.find("</body>").unwrap());
    }

    #[test]
    fn yenileme_betigi_body_yoksa_sona_eklenir() {
        let sonuc = betigi_ekle("<p>a</p>");
        assert!(sonuc.contains("__siteturk/durum"));
        assert!(sonuc.starts_with("<p>a</p>"));
    }

    #[test]
    fn kok_altinda_dogru_dogru_isler() {
        let dizin = GeciciDizin::yeni("altinda").unwrap();
        let yol = dizin.yaz("a.txt", "x").unwrap();
        assert!(kok_altinda(&dizin.yol, &yol));
        let dis = std::env::temp_dir();
        assert!(!kok_altinda(&dizin.yol, &dis));
    }

    #[test]
    fn dosya_sunulur_ve_betik_enjekte_edilir() {
        let dizin = GeciciDizin::yeni("dosya").unwrap();
        dizin
            .yaz("index.html", "<html><body>Merhaba</body></html>")
            .unwrap();
        let mut sunucu = Sunucu::baslat(&dizin.yol, 0).unwrap();
        let istek = Istek {
            yontem: "GET".to_string(),
            yol: "/index.html".to_string(),
            sorgu: String::new(),
            surum: "HTTP/1.1".to_string(),
        };
        let yanit = sunucu.yanit_uret(&istek);
        assert_eq!(yanit.kod, 200);
        let metin = String::from_utf8_lossy(&yanit.govde);
        assert!(metin.contains("Merhaba"));
        assert!(metin.contains("__siteturk/durum"));
    }

    #[test]
    fn kok_disina_cikmaya_calisan_istek_403_verir() {
        let dizin = GeciciDizin::yeni("kacis").unwrap();
        dizin.yaz("index.html", "<p>x</p>").unwrap();
        let mut sunucu = Sunucu::baslat(&dizin.yol, 0).unwrap();
        let istek = Istek {
            yontem: "GET".to_string(),
            yol: "/../../gizli.txt".to_string(),
            sorgu: String::new(),
            surum: "HTTP/1.1".to_string(),
        };
        let yanit = sunucu.yanit_uret(&istek);
        assert!(yanit.kod == 400 || yanit.kod == 404, "kod: {}", yanit.kod);
        assert!(!String::from_utf8_lossy(&yanit.govde).contains("root:"));
    }

    #[test]
    fn olmayan_dosya_404_verir() {
        let dizin = GeciciDizin::yeni("yok").unwrap();
        let mut sunucu = Sunucu::baslat(&dizin.yol, 0).unwrap();
        let istek = Istek {
            yontem: "GET".to_string(),
            yol: "/yok.html".to_string(),
            sorgu: String::new(),
            surum: "HTTP/1.1".to_string(),
        };
        assert_eq!(sunucu.yanit_uret(&istek).kod, 404);
    }

    #[test]
    fn yazma_yontemi_405_verir() {
        let dizin = GeciciDizin::yeni("yazma").unwrap();
        let mut sunucu = Sunucu::baslat(&dizin.yol, 0).unwrap();
        let istek = Istek {
            yontem: "POST".to_string(),
            yol: "/".to_string(),
            sorgu: String::new(),
            surum: "HTTP/1.1".to_string(),
        };
        assert_eq!(sunucu.yanit_uret(&istek).kod, 405);
    }

    #[test]
    fn durum_uclusu_uretim_sayacini_dondurur() {
        let dizin = GeciciDizin::yeni("durum").unwrap();
        let mut sunucu = Sunucu::baslat(&dizin.yol, 0).unwrap();
        let istek = Istek {
            yontem: "GET".to_string(),
            yol: DURUM_YOLU.to_string(),
            sorgu: "k=0".to_string(),
            surum: "HTTP/1.1".to_string(),
        };
        let ilk = sunucu.yanit_uret(&istek);
        assert_eq!(ilk.kod, 200);
        assert_eq!(String::from_utf8_lossy(&ilk.govde), "0");
        sunucu.uretim_artir();
        let ikinci = sunucu.yanit_uret(&istek);
        assert_eq!(String::from_utf8_lossy(&ikinci.govde), "1");
    }

    #[test]
    fn turkce_dosya_adi_sunulur() {
        let dizin = GeciciDizin::yeni("turkce").unwrap();
        dizin.yaz("hakkımızda/index.html", "<p>Türkçe</p>").unwrap();
        let mut sunucu = Sunucu::baslat(&dizin.yol, 0).unwrap();
        // Yüzde kaçışının çözülmesi `istek_ayikla`'nın işidir; burada elde edilen
        // (çözülmüş) yol doğrudan dosya okuyucuya verilir.
        let istek = Istek {
            yontem: "GET".to_string(),
            yol: "/hakkımızda/".to_string(),
            sorgu: String::new(),
            surum: "HTTP/1.1".to_string(),
        };
        let yanit = sunucu.yanit_uret(&istek);
        assert_eq!(yanit.kod, 200, "kod: {}", yanit.kod);
        assert!(String::from_utf8_lossy(&yanit.govde).contains("Türkçe"));
    }

    #[test]
    fn yuzden_yazilen_turkce_yol_gercek_dosyaya_ulasir() {
        let dizin = GeciciDizin::yeni("turkce2").unwrap();
        dizin.yaz("hakkımızda/index.html", "<p>Türkçe</p>").unwrap();
        let ham = istek_ayikla("GET /hakk%C4%B1m%C4%B1zda/ HTTP/1.1\r\n\r\n").unwrap();
        let mut sunucu = Sunucu::baslat(&dizin.yol, 0).unwrap();
        let yanit = sunucu.yanit_uret(&ham);
        assert_eq!(yanit.kod, 200, "kod: {}", yanit.kod);
    }

    #[test]
    fn sunucu_yalniz_yerel_adrese_baglanir() {
        let dizin = GeciciDizin::yeni("yerel").unwrap();
        let sunucu = Sunucu::baslat(&dizin.yol, 0).unwrap();
        assert!(
            sunucu.yerel_adres().starts_with("127.0.0.1:"),
            "adres: {}",
            sunucu.yerel_adres()
        );
    }

    #[test]
    fn css_dosyasi_betik_enjekte_edilmez() {
        let dizin = GeciciDizin::yeni("css").unwrap();
        dizin.yaz("a.css", "body{color:red}").unwrap();
        let mut sunucu = Sunucu::baslat(&dizin.yol, 0).unwrap();
        let istek = Istek {
            yontem: "GET".to_string(),
            yol: "/a.css".to_string(),
            sorgu: String::new(),
            surum: "HTTP/1.1".to_string(),
        };
        let yanit = sunucu.yanit_uret(&istek);
        assert_eq!(yanit.kod, 200);
        assert_eq!(yanit.icerik_tipi, "text/css; charset=utf-8");
        assert!(!String::from_utf8_lossy(&yanit.govde).contains("__siteturk"));
    }
}
