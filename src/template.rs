//! Sığ şablon dili: `{{ degisken }}`, `{% if %}`, `{% for %}`.
//!
//! Dil bilinçli olarak küçüktür (rapor § 07, "Şablon dili ve derleyici kararı").
//! Fonksiyon tanımı, dosya çağırma, aritmetik ve süzgeç zinciri **yoktur**;
//! karmaşıklık şablondan üreticiye taşınmıştır. Yanlış yazılmış bir değişken
//! adının boş dizeye dönüşmesi engellenir: tanımsız değişken **hata** üretir.
//!
//! Güvenlik modeli:
//!
//! * Varsayılan çıktı **kaçışlıdır**; `{{ }}` içindeki değer HTML'e güvenle girer.
//! * Ham HTML yalnızca açık `| ham` süzgeci veya `ham()` çağrısı ile yazılır.
//! * Döngü adımı, iç içe geçiş derinliği ve çıktı boyutu sınırlıdır; bu
//!   sınırlar "kötü niyetli şablonun süreci tüketmesi" saldırısını kapatır.

use std::collections::BTreeMap;
use std::fmt;

use crate::error::Hata;

/// İç içe geçmiş blokların azami derinliği.
pub const MAX_DERINLIK: usize = 32;

/// Tek bir döngünün azami yineleme sayısı.
pub const MAX_DONGU_ADIMI: usize = 10_000;

/// Üretilebilecek azami çıktı boyutu (bayt).
pub const MAX_CIKTI_BAYT: usize = 32 * 1024 * 1024;

/// Şablonun çalıştırılması sırasında ortaya çıkan hata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SablonHatasi {
    /// `{{` ya da `{%` açıldı ama kapatılmadı.
    KapanmayanIsaret {
        /// Hatanın geçtiği satır.
        satir: usize,
        /// Açılan işaretin metni.
        isaret: String,
    },
    /// Tanınmayan etiket adı.
    BilinmeyenEtiket {
        /// Hatanın geçtiği satır.
        satir: usize,
        /// Etiket adı.
        etiket: String,
    },
    /// `{% endif %}` ya da `{% endfor %}` eşleşmesiz.
    EslesmeyenKapanis {
        /// Hatanın geçtiği satır.
        satir: usize,
        /// Kapanış etiketinin adı.
        etiket: String,
    },
    /// Bloğun kapanışı eksik.
    KapanmamisBlok {
        /// Hatanın geçtiği satır.
        satir: usize,
        /// Açılan bloğun etiketi.
        etiket: String,
    },
    /// Değişken yolu boş ya da geçersiz.
    GecersizYol {
        /// Hatanın geçtiği satır.
        satir: usize,
        /// Yazılan yol.
        yol: String,
    },
    /// Bağlamda bulunmayan değişken.
    TanimsizDegisken {
        /// Hatanın geçtiği satır.
        satir: usize,
        /// Aranan yol.
        yol: String,
    },
    /// Tanınmayan işlev adı.
    BilinmeyenIslev {
        /// Hatanın geçtiği satır.
        satir: usize,
        /// Yazılan işlev adı.
        islev: String,
    },
    /// İşlevin yanlış sayıda ya da yanlış türde argüman aldığı.
    BozukArguman {
        /// Hatanın geçtiği satır.
        satir: usize,
        /// Açıklama.
        mesaj: String,
    },
    /// İç içe geçiş sınırı aşıldı.
    DerinlikSiniri {
        /// Hatanın geçtiği satır.
        satir: usize,
    },
    /// Döngü sınırı aşıldı.
    DonguSiniri {
        /// Hatanın geçtiği satır.
        satir: usize,
    },
    /// Üretilen çıktı sınırı aştı.
    CiktiSiniri,
}

impl fmt::Display for SablonHatasi {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SablonHatasi::KapanmayanIsaret { satir, isaret } => {
                write!(f, "satir {satir}: `{isaret}` isareti kapatilmadi")
            }
            SablonHatasi::BilinmeyenEtiket { satir, etiket } => {
                write!(f, "satir {satir}: bilinmeyen etiket `{etiket}`")
            }
            SablonHatasi::EslesmeyenKapanis { satir, etiket } => {
                write!(f, "satir {satir}: `{etiket}` etiketi icin acilis yok")
            }
            SablonHatasi::KapanmamisBlok { satir, etiket } => {
                write!(f, "satir {satir}: `{etiket}` blogu kapatilmadi")
            }
            SablonHatasi::GecersizYol { satir, yol } => {
                write!(f, "satir {satir}: gecersiz degisken yolu `{yol}`")
            }
            SablonHatasi::TanimsizDegisken { satir, yol } => {
                write!(f, "satir {satir}: tanimsiz degisken `{yol}`")
            }
            SablonHatasi::BilinmeyenIslev { satir, islev } => {
                write!(f, "satir {satir}: bilinmeyen islev `{islev}`")
            }
            SablonHatasi::BozukArguman { satir, mesaj } => {
                write!(f, "satir {satir}: {mesaj}")
            }
            SablonHatasi::DerinlikSiniri { satir } => write!(
                f,
                "satir {satir}: sablon ic ice gecis siniri ({MAX_DERINLIK}) asildi"
            ),
            SablonHatasi::DonguSiniri { satir } => {
                write!(f, "satir {satir}: dongu siniri ({MAX_DONGU_ADIMI}) asildi")
            }
            SablonHatasi::CiktiSiniri => {
                write!(f, "sablon ciktisi siniri ({MAX_CIKTI_BAYT} bayt) asildi")
            }
        }
    }
}

impl std::error::Error for SablonHatasi {}

/// Şablonun çalıştırılabilir hâlinden çıkan ve bağlama konulan değer.
#[derive(Debug, Clone, PartialEq)]
pub enum Deger {
    /// Boş değer.
    Bos,
    /// Metin.
    Metin(String),
    /// Tam sayı.
    Sayi(i64),
    /// Doğru.
    Dogru,
    /// Yanlış.
    Yanlis,
    /// Liste.
    Liste(Vec<Deger>),
    /// Anahtar/değer haritası.
    Harita(BTreeMap<String, Deger>),
}

impl Deger {
    /// Metin değeri üretir.
    pub fn metin(deger: impl Into<String>) -> Self {
        Deger::Metin(deger.into())
    }

    /// Liste değeri üretir.
    pub fn liste(degerler: impl IntoIterator<Item = Deger>) -> Self {
        Deger::Liste(degerler.into_iter().collect())
    }

    /// Harita değeri üretir.
    pub fn harita(anahtarlar: impl IntoIterator<Item = (String, Deger)>) -> Self {
        Deger::Harita(anahtarlar.into_iter().collect())
    }

    /// Değerin yol gösterimindeki karşılığını arar.
    ///
    /// `a.b.c` yolunda `a` bir harita ise `b` çıktısına bakılır; liste ise
    /// sayısal indeks desteklenmez ve arama başarısız olur.
    pub fn ara(&self, yol: &str) -> Option<&Deger> {
        let mut mevcut = self;
        for parca in yol.split('.') {
            match mevcut {
                Deger::Harita(harita) => mevcut = harita.get(parca)?,
                Deger::Liste(liste) => {
                    let indeks: usize = parca.parse().ok()?;
                    mevcut = liste.get(indeks)?;
                }
                _ => return None,
            }
        }
        Some(mevcut)
    }

    /// Koşullu ifadede doğruluk değerini verir.
    ///
    /// Boş metin, `0` ve boş liste yanlış kabul edilir; bu, şablon yazımını
    /// kısa tutar (`{% if sayfa.ozet %}` her yerde çalışır).
    pub fn dogru_mu(&self) -> bool {
        match self {
            Deger::Bos | Deger::Yanlis => false,
            Deger::Dogru => true,
            Deger::Metin(metin) => !metin.is_empty(),
            Deger::Sayi(sayi) => *sayi != 0,
            Deger::Liste(liste) => !liste.is_empty(),
            Deger::Harita(harita) => !harita.is_empty(),
        }
    }

    /// Değeri kaçışlı metne çevirir.
    pub fn metin_kacisli(&self) -> String {
        match self {
            Deger::Bos => String::new(),
            Deger::Metin(metin) => metin.clone(),
            Deger::Sayi(sayi) => sayi.to_string(),
            Deger::Dogru => "true".to_string(),
            Deger::Yanlis => "false".to_string(),
            Deger::Liste(_) | Deger::Harita(_) => String::new(),
        }
    }

    /// Değeri ham metne çevirir (yalnız `ham` süzgeciyle kullanılır).
    pub fn metin_ham(&self) -> String {
        self.metin_kacisli()
    }

    /// Değer liste olarak yorumlanabiliyorsa döngüye uygundur.
    pub fn liste_mi(&self) -> Option<&[Deger]> {
        match self {
            Deger::Liste(liste) => Some(liste),
            Deger::Harita(harita) => {
                // Harita, `for anahtar, deger in ...` yerine tek değişkenli
                // döngüde değer dizisi olarak gezilir.
                let _ = harita;
                None
            }
            _ => None,
        }
    }
}

/// Metni HTML metin düğümü için kaçışlar.
pub fn kacis(girdi: &str) -> String {
    let mut cikti = String::with_capacity(girdi.len());
    for karakter in girdi.chars() {
        match karakter {
            '&' => cikti.push_str("&amp;"),
            '<' => cikti.push_str("&lt;"),
            '>' => cikti.push_str("&gt;"),
            '"' => cikti.push_str("&quot;"),
            '\'' => cikti.push_str("&#39;"),
            _ => cikti.push(karakter),
        }
    }
    cikti
}

/// Bir ifadenin ayrıştırılmış hâli.
#[derive(Debug, Clone, PartialEq)]
enum Ifade {
    /// `a.b.c` biçiminde bir yol.
    Yol(String),
    /// `5` biçiminde bir tam sayı sabiti.
    Sayi(i64),
    /// `varlik("yol")` ya da `ham(deger)` biçiminde işlev çağrısı.
    Cagri {
        islev: String,
        argumanlar: Vec<Arguman>,
    },
}

impl Ifade {
    fn metin(&self) -> String {
        match self {
            Ifade::Yol(yol) => yol.clone(),
            Ifade::Sayi(sayi) => sayi.to_string(),
            Ifade::Cagri { islev, .. } => format!("{islev}(...)"),
        }
    }
}

/// Çağrının tek bir argümanı.
#[derive(Debug, Clone, PartialEq)]
enum Arguman {
    /// Tırnak içinde yazılmış sabit metin.
    Metin(String),
    /// Değişken yolu ya da iç içe çağrı.
    Ifade(Ifade),
}

/// Derlenmiş şablonun tek bir düğümü.
#[derive(Debug, Clone, PartialEq)]
enum Dugum {
    Metin(String),
    Degisken {
        ifade: Ifade,
        ham: bool,
        satir: usize,
    },
    If {
        kosul: Ifade,
        dogrusal: Vec<Dugum>,
        alternatif: Vec<Dugum>,
        satir: usize,
    },
    For {
        degisken: String,
        kaynak: Ifade,
        govde: Vec<Dugum>,
        satir: usize,
    },
}

/// Bir şablon dosyasının derlenmiş hâli.
///
/// Derleme bir kez yapılır, çalıştırma defalarca yapılabilir; bu, canlı
/// yenileme sunucusunda şablon dizesinin yeniden ayrıştırılmamasını sağlar.
#[derive(Debug, Clone, PartialEq)]
pub struct Sablon {
    dugumler: Vec<Dugum>,
    ad: String,
}

/// Sıralanmamış eşleşiklerin ayrıştırılmış hâli.
enum Parca {
    Duz(String),
    Degisken(String, usize),
    Etiket(String, usize),
}

impl Sablon {
    /// Şablon dizesini ayrıştırır ve çalıştırılabilir hâle getirir.
    ///
    /// # Hatalar
    ///
    /// Söz dizimi hataları `Hata::Sablon` olarak, dosya ve satır bilgisiyle döner.
    pub fn ayikla(dosya_adi: &str, kaynak: &str) -> Result<Self, Hata> {
        let parcalar = parcalara_ayir(dosya_adi, kaynak)?;
        let mut indeks = 0usize;
        let mut dugumler: Vec<Dugum> = Vec::new();
        let mut yigin: Vec<AcikBlok> = Vec::new();
        bloklari_ayikla(dosya_adi, &parcalar, &mut indeks, &mut yigin, &mut dugumler)?;
        if let Some(blok) = yigin.first() {
            return Err(Hata::sablon(
                dosya_adi,
                blok.satir(),
                format!("`{}` blogu kapatilmadi", blok.etiket()),
            ));
        }
        if let Some(Parca::Etiket(govde, satir)) = parcalar.get(indeks) {
            return Err(Hata::sablon(
                dosya_adi,
                *satir,
                format!("`{}` etiketi icin acilis yok", govde.trim()),
            ));
        }
        Ok(Sablon {
            dugumler,
            ad: dosya_adi.to_string(),
        })
    }
}

/// Henüz kapanmamış bir `{% if %}` veya `{% for %}` bloğu.
#[derive(Debug, Clone)]
enum AcikBlok {
    If {
        baslangic: usize,
        kosul: Ifade,
        satir: usize,
        dogrusal: Vec<Dugum>,
        alternatif: Option<usize>,
    },
    For {
        baslangic: usize,
        degisken: String,
        kaynak: Ifade,
        satir: usize,
    },
}

impl AcikBlok {
    /// Bloğun açıldığı satır.
    fn satir(&self) -> usize {
        match self {
            AcikBlok::If { satir, .. } | AcikBlok::For { satir, .. } => *satir,
        }
    }

    /// Bloğun etiket adı.
    fn etiket(&self) -> &'static str {
        match self {
            AcikBlok::If { .. } => "if",
            AcikBlok::For { .. } => "for",
        }
    }
}

/// Parça listesini özyinelemeli olarak düğümlere çevirir.
fn bloklari_ayikla(
    dosya_adi: &str,
    parcalar: &[Parca],
    indeks: &mut usize,
    yigin: &mut Vec<AcikBlok>,
    cikti: &mut Vec<Dugum>,
) -> Result<(), Hata> {
    while *indeks < parcalar.len() {
        match &parcalar[*indeks] {
            Parca::Duz(metin) => {
                cikti.push(Dugum::Metin(metin.clone()));
                *indeks += 1;
            }
            Parca::Degisken(govde, satir) => {
                cikti.push(degisken_dugumu(dosya_adi, *satir, govde)?);
                *indeks += 1;
            }
            Parca::Etiket(govde, satir) => {
                let satir = *satir;
                let takim: Vec<&str> = govde.split_whitespace().collect();
                let ad = takim.first().copied().unwrap_or("").to_string();
                *indeks += 1;
                match ad.as_str() {
                    "if" => {
                        if takim.len() != 2 {
                            return Err(Hata::sablon(
                                dosya_adi,
                                satir,
                                "`if` bicimi: {% if degisken %}",
                            ));
                        }
                        let kosul = ifade_ayikla(dosya_adi, satir, takim[1])?;
                        yigin.push(AcikBlok::If {
                            baslangic: cikti.len(),
                            kosul,
                            satir,
                            dogrusal: Vec::new(),
                            alternatif: None,
                        });
                    }
                    "else" => {
                        if takim.len() != 1 {
                            return Err(Hata::sablon(
                                dosya_adi,
                                satir,
                                "`else` arguman almaz: {% else %}",
                            ));
                        }
                        match yigin.last_mut() {
                            Some(AcikBlok::If {
                                baslangic,
                                dogrusal,
                                alternatif,
                                ..
                            }) if alternatif.is_none() => {
                                // `if` dalı kesilip yığına taşınır; `else` dalı
                                // çıktı tamponunun sonunda birikir.
                                *dogrusal = cikti.drain(*baslangic..).collect();
                                *alternatif = Some(cikti.len());
                            }
                            Some(AcikBlok::If { .. }) => {
                                return Err(Hata::sablon(
                                    dosya_adi,
                                    satir,
                                    "bir `if` blogu icinde yalnizca bir `else` kullanilabilir",
                                ))
                            }
                            _ => {
                                return Err(Hata::sablon(
                                    dosya_adi,
                                    satir,
                                    "`else` yalnizca `if` blogu icinde kullanilabilir",
                                ))
                            }
                        }
                    }
                    "endif" => {
                        if takim.len() != 1 {
                            return Err(Hata::sablon(
                                dosya_adi,
                                satir,
                                "`endif` arguman almaz: {% endif %}",
                            ));
                        }
                        match yigin.pop() {
                            Some(AcikBlok::If {
                                baslangic,
                                kosul,
                                satir: _,
                                dogrusal,
                                alternatif,
                            }) => {
                                let (dogrusal, alternatif_dugumler) = match alternatif {
                                    Some(alt) => (
                                        dogrusal,
                                        cikti.drain(alt..).collect::<Vec<Dugum>>(),
                                    ),
                                    None => (cikti.drain(baslangic..).collect(), Vec::new()),
                                };
                                cikti.push(Dugum::If {
                                    kosul,
                                    dogrusal,
                                    alternatif: alternatif_dugumler,
                                    satir,
                                });
                            }
                            Some(blok) => {
                                let etiket = blok.etiket();
                                yigin.push(blok);
                                return Err(Hata::sablon(
                                    dosya_adi,
                                    satir,
                                    format!("`endif` etiketi `{etiket}` blogunu kapatti"),
                                ));
                            }
                            None => {
                                return Err(Hata::sablon(
                                    dosya_adi,
                                    satir,
                                    "`endif` etiketi icin acilis yok",
                                ))
                            }
                        }
                    }
                    "for" => {
                        if takim.len() != 4 || takim[2] != "in" {
                            return Err(Hata::sablon(
                                dosya_adi,
                                satir,
                                "`for` bicimi: {% for degisken in dizi %}",
                            ));
                        }
                        let degisken = takim[1].to_string();
                        if degisken.is_empty()
                            || !degisken
                                .chars()
                                .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
                        {
                            return Err(Hata::sablon(
                                dosya_adi,
                                satir,
                                format!("gecersiz dongu degiskeni `{degisken}`"),
                            ));
                        }
                        let kaynak = ifade_ayikla(dosya_adi, satir, takim[3])?;
                        yigin.push(AcikBlok::For {
                            baslangic: cikti.len(),
                            degisken,
                            kaynak,
                            satir,
                        });
                    }
                    "endfor" => {
                        if takim.len() != 1 {
                            return Err(Hata::sablon(
                                dosya_adi,
                                satir,
                                "`endfor` arguman almaz: {% endfor %}",
                            ));
                        }
                        match yigin.pop() {
                            Some(AcikBlok::For {
                                baslangic,
                                degisken,
                                kaynak,
                                ..
                            }) => {
                                let govde: Vec<Dugum> = cikti.drain(baslangic..).collect();
                                cikti.push(Dugum::For {
                                    degisken,
                                    kaynak,
                                    govde,
                                    satir,
                                });
                            }
                            Some(blok) => {
                                let etiket = blok.etiket();
                                yigin.push(blok);
                                return Err(Hata::sablon(
                                    dosya_adi,
                                    satir,
                                    format!("`endfor` etiketi `{etiket}` blogunu kapatti"),
                                ));
                            }
                            None => {
                                return Err(Hata::sablon(
                                    dosya_adi,
                                    satir,
                                    "`endfor` etiketi icin acilis yok",
                                ))
                            }
                        }
                    }
                    "" => {
                        return Err(Hata::sablon(
                            dosya_adi,
                            satir,
                            "bos etiket; `{% if %}`, `{% else %}`, `{% endif %}`, `{% for %}`, `{% endfor %}` bekleniyordu",
                        ))
                    }
                    diger => {
                        return Err(Hata::sablon(
                            dosya_adi,
                            satir,
                            format!(
                                "bilinmeyen etiket `{diger}`; dilde yalnizca if/else/endif/for/endfor vardir"
                            ),
                        ))
                    }
                }
            }
        }
    }
    Ok(())
}

/// Şablonu bağlamla çalıştırır.
pub fn isle(dosya_adi: &str, kaynak: &str, baglam: &Deger) -> Result<String, Hata> {
    let sablon = Sablon::ayikla(dosya_adi, kaynak)?;
    sablon.calistir(baglam)
}

/// Şablon dizesini `{{ }}` ve `{% %}` sınırlarına göre parçalar.
fn parcalara_ayir(dosya_adi: &str, kaynak: &str) -> Result<Vec<Parca>, Hata> {
    let karakterler: Vec<char> = kaynak.chars().collect();
    let mut parcalar = Vec::new();
    let mut duz = String::new();
    let mut i = 0usize;
    let mut satir = 1usize;

    while i < karakterler.len() {
        let c = karakterler[i];
        if c == '\n' {
            satir += 1;
        }
        if (c == '{' && karakterler.get(i + 1) == Some(&'{'))
            || (c == '{' && karakterler.get(i + 1) == Some(&'%'))
        {
            let kapanis: [char; 2] = if c == '{' && karakterler.get(i + 1) == Some(&'{') {
                ['}', '}']
            } else {
                ['%', '}']
            };
            let acilis = if kapanis[0] == '}' { "{{" } else { "{%" };
            if !duz.is_empty() {
                parcalar.push(Parca::Duz(std::mem::take(&mut duz)));
            }
            let mut j = i + 2;
            let mut govde = String::new();
            let mut bulundu = false;
            while j < karakterler.len() {
                if karakterler[j] == kapanis[0] && karakterler[j + 1] == kapanis[1] {
                    bulundu = true;
                    break;
                }
                if karakterler[j] == '\n' {
                    satir += 1;
                }
                govde.push(karakterler[j]);
                j += 1;
            }
            if !bulundu {
                return Err(Hata::sablon(
                    dosya_adi,
                    satir,
                    format!("`{acilis}` isareti kapatilmadi"),
                ));
            }
            if kapanis[0] == '}' {
                parcalar.push(Parca::Degisken(govde.trim().to_string(), satir));
            } else {
                parcalar.push(Parca::Etiket(govde.trim().to_string(), satir));
            }
            i = j + 2;
            continue;
        }
        duz.push(c);
        i += 1;
    }
    if !duz.is_empty() {
        parcalar.push(Parca::Duz(duz));
    }
    Ok(parcalar)
}

impl Sablon {
    /// Derlenmiş şablonu verilen bağlamla çalıştırır.
    pub fn calistir(&self, baglam: &Deger) -> Result<String, Hata> {
        let mut cikti = String::new();
        let mut durum = CalistirmaDurumu { adim: 0 };
        self.dugumleri_calistir(&self.dugumler, baglam, 0, &mut cikti, &mut durum)?;
        Ok(cikti)
    }

    /// Düğüm listesini çalıştırır.
    fn dugumleri_calistir(
        &self,
        dugumler: &[Dugum],
        baglam: &Deger,
        derinlik: usize,
        cikti: &mut String,
        durum: &mut CalistirmaDurumu,
    ) -> Result<(), Hata> {
        if derinlik > MAX_DERINLIK {
            return Err(self.hata(SablonHatasi::DerinlikSiniri { satir: 1 }));
        }
        for dugum in dugumler {
            match dugum {
                Dugum::Metin(metin) => {
                    if !ekle(cikti, metin) {
                        return Err(self.hata(SablonHatasi::CiktiSiniri));
                    }
                }
                Dugum::Degisken { ifade, ham, satir } => {
                    let deger = self.ifadeyi_coz(ifade, *satir, baglam, durum)?;
                    let metin = if *ham {
                        deger.metin_ham()
                    } else {
                        kacis(&deger.metin_kacisli())
                    };
                    if !ekle(cikti, &metin) {
                        return Err(self.hata(SablonHatasi::CiktiSiniri));
                    }
                }
                Dugum::If {
                    kosul,
                    dogrusal,
                    alternatif,
                    satir,
                } => {
                    let deger = self.ifadeyi_coz(kosul, *satir, baglam, durum)?;
                    let secilecek = if deger.dogru_mu() {
                        dogrusal
                    } else {
                        alternatif
                    };
                    self.dugumleri_calistir(secilecek, baglam, derinlik + 1, cikti, durum)?;
                }
                Dugum::For {
                    degisken,
                    kaynak,
                    govde,
                    satir,
                } => {
                    let deger = self.ifadeyi_coz(kaynak, *satir, baglam, durum)?;
                    let ogeler: Vec<Deger> = match &deger {
                        Deger::Liste(liste) => liste.clone(),
                        Deger::Harita(harita) => harita.values().cloned().collect(),
                        Deger::Bos => Vec::new(),
                        diger => vec![diger.clone()],
                    };
                    if ogeler.len() > MAX_DONGU_ADIMI {
                        return Err(self.hata(SablonHatasi::DonguSiniri { satir: *satir }));
                    }
                    for oge in ogeler {
                        durum.adim += 1;
                        if durum.adim > MAX_DONGU_ADIMI {
                            return Err(self.hata(SablonHatasi::DonguSiniri { satir: *satir }));
                        }
                        let mut alt = baglam.clone();
                        if let Deger::Harita(harita) = &mut alt {
                            harita.insert(degisken.clone(), oge);
                        }
                        self.dugumleri_calistir(govde, &alt, derinlik + 1, cikti, durum)?;
                    }
                }
            }
        }
        Ok(())
    }

    /// İfadeyi bağlamda çözer.
    fn ifadeyi_coz(
        &self,
        ifade: &Ifade,
        satir: usize,
        baglam: &Deger,
        durum: &CalistirmaDurumu,
    ) -> Result<Deger, Hata> {
        match ifade {
            Ifade::Yol(yol) => {
                if yol.is_empty() {
                    return Err(self.hata(SablonHatasi::GecersizYol {
                        satir,
                        yol: yol.clone(),
                    }));
                }
                baglam.ara(yol).cloned().ok_or_else(|| {
                    self.hata(SablonHatasi::TanimsizDegisken {
                        satir,
                        yol: yol.clone(),
                    })
                })
            }
            Ifade::Sayi(sayi) => Ok(Deger::Sayi(*sayi)),
            Ifade::Cagri { islev, argumanlar } => {
                self.cagriyi_coz(islev, argumanlar, satir, baglam, durum)
            }
        }
    }

    /// Yerleşik işlevleri çalıştırır.
    fn cagriyi_coz(
        &self,
        islev: &str,
        argumanlar: &[Arguman],
        satir: usize,
        baglam: &Deger,
        durum: &CalistirmaDurumu,
    ) -> Result<Deger, Hata> {
        let mut cozulmus: Vec<Deger> = Vec::with_capacity(argumanlar.len());
        for arguman in argumanlar {
            let deger = match arguman {
                Arguman::Metin(metin) => Deger::Metin(metin.clone()),
                Arguman::Ifade(ifade) => self.ifadeyi_coz(ifade, satir, baglam, durum)?,
            };
            cozulmus.push(deger);
        }
        match islev {
            "ham" => {
                if cozulmus.len() != 1 {
                    return Err(self.hata(SablonHatasi::BozukArguman {
                        satir,
                        mesaj: "`ham` tam olarak 1 arguman ister".to_string(),
                    }));
                }
                Ok(cozulmus[0].clone())
            }
            "varlik" => {
                if cozulmus.len() != 1 {
                    return Err(self.hata(SablonHatasi::BozukArguman {
                        satir,
                        mesaj: "`varlik` tam olarak 1 arguman ister".to_string(),
                    }));
                }
                // `site.varliklar` haritası imzalı adresleri taşır. Anahtar
                // adı nokta içerebildiği için (`logo.png`) noktaya göre yol
                // ayrıştırılmaz; doğrudan harita anahtarı olarak bakılır.
                let ad = cozulmus[0].metin_kacisli();
                let cozulmus_adres = match baglam.ara("site.varliklar") {
                    Some(Deger::Harita(harita)) => harita
                        .get(&ad)
                        .map(|deger| deger.metin_kacisli())
                        .unwrap_or_else(|| format!("/static/{ad}")),
                    _ => format!("/static/{ad}"),
                };
                Ok(Deger::Metin(cozulmus_adres))
            }
            "kisalt" => {
                if cozulmus.len() != 2 {
                    return Err(self.hata(SablonHatasi::BozukArguman {
                        satir,
                        mesaj: "`kisalt` 2 arguman ister: kisalt(metin, uzunluk)".to_string(),
                    }));
                }
                let sinir = match &cozulmus[1] {
                    Deger::Sayi(sayi) if *sayi >= 0 => *sayi as usize,
                    _ => {
                        return Err(self.hata(SablonHatasi::BozukArguman {
                            satir,
                            mesaj: "`kisalt` ikinci argumani tam sayi olmali".to_string(),
                        }))
                    }
                };
                let metin = cozulmus[0].metin_kacisli();
                let kisaltilmis: String = if metin.chars().count() <= sinir {
                    metin
                } else {
                    let ilk: String = metin.chars().take(sinir.saturating_sub(1)).collect();
                    format!("{ilk}...")
                };
                Ok(Deger::Metin(kisaltilmis))
            }
            diger => Err(self.hata(SablonHatasi::BilinmeyenIslev {
                satir,
                islev: diger.to_string(),
            })),
        }
    }

    /// Şablon hatasını dosya adıyla sarmalar.
    fn hata(&self, hata: SablonHatasi) -> Hata {
        let satir = match &hata {
            SablonHatasi::KapanmayanIsaret { satir, .. }
            | SablonHatasi::BilinmeyenEtiket { satir, .. }
            | SablonHatasi::EslesmeyenKapanis { satir, .. }
            | SablonHatasi::KapanmamisBlok { satir, .. }
            | SablonHatasi::GecersizYol { satir, .. }
            | SablonHatasi::TanimsizDegisken { satir, .. }
            | SablonHatasi::BilinmeyenIslev { satir, .. }
            | SablonHatasi::BozukArguman { satir, .. }
            | SablonHatasi::DerinlikSiniri { satir }
            | SablonHatasi::DonguSiniri { satir } => *satir,
            SablonHatasi::CiktiSiniri => 0,
        };
        Hata::Sablon {
            dosya: self.ad.clone(),
            satir,
            mesaj: hata.to_string(),
        }
    }
}

/// Çalıştırma sırasında paylaşılan sayaç.
struct CalistirmaDurumu {
    adim: usize,
}

/// Çıktı tamponuna ekler; sınır aşılırsa `false` döner.
fn ekle(cikti: &mut String, metin: &str) -> bool {
    if cikti.len().saturating_add(metin.len()) > MAX_CIKTI_BAYT {
        return false;
    }
    cikti.push_str(metin);
    true
}

/// `{{ ifade }}` gövdesini ayrıştırır.
fn degisken_dugumu(dosya_adi: &str, satir: usize, govde: &str) -> Result<Dugum, Hata> {
    let (ifade, ham) = ifade_ve_suzgeci_ayikla(dosya_adi, satir, govde)?;
    Ok(Dugum::Degisken { ifade, ham, satir })
}

/// `a.b.c | ham` gövdesini ifade ve süzgeç olarak ayırır.
fn ifade_ve_suzgeci_ayikla(
    dosya_adi: &str,
    satir: usize,
    govde: &str,
) -> Result<(Ifade, bool), Hata> {
    let parcalar: Vec<&str> = govde.split('|').map(|p| p.trim()).collect();
    if parcalar.is_empty() {
        return Err(Hata::sablon(dosya_adi, satir, "bos degisken ifadesi"));
    }
    let ifade = ifade_ayikla(dosya_adi, satir, parcalar[0])?;
    let mut ham = false;
    for suzgec in &parcalar[1..] {
        match *suzgec {
            "ham" => ham = true,
            diger => {
                return Err(Hata::sablon(
                    dosya_adi,
                    satir,
                    format!("bilinmeyen suzgec `{diger}`; yalnizca `ham` kullanilabilir"),
                ))
            }
        }
    }
    Ok((ifade, ham))
}

/// Bir yolu ya da işlev çağrısını ayrıştırır.
fn ifade_ayikla(dosya_adi: &str, satir: usize, metin: &str) -> Result<Ifade, Hata> {
    let metin = metin.trim();
    if metin.is_empty() {
        return Err(Hata::sablon(dosya_adi, satir, "bos ifade"));
    }
    if let Some(parantez) = metin.find('(') {
        if !metin.ends_with(')') {
            return Err(Hata::sablon(
                dosya_adi,
                satir,
                format!("cagri `{}` kapanmiyor", &metin[..parantez]),
            ));
        }
        let islev = metin[..parantez].trim().to_string();
        if islev.is_empty() || !islev.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Err(Hata::sablon(
                dosya_adi,
                satir,
                format!("gecersiz islev adi `{islev}`"),
            ));
        }
        let ic = &metin[parantez + 1..metin.len() - 1];
        let argumanlar = if ic.trim().is_empty() {
            Vec::new()
        } else {
            let mut liste = Vec::new();
            let mut derinlik = 0usize;
            let mut mevcut = String::new();
            let mut tirnak: Option<char> = None;
            for karakter in ic.chars() {
                match tirnak {
                    Some(t) => {
                        mevcut.push(karakter);
                        if karakter == t {
                            tirnak = None;
                        }
                    }
                    None => match karakter {
                        '"' | '\'' => {
                            tirnak = Some(karakter);
                            mevcut.push(karakter);
                        }
                        ',' if derinlik == 0 => {
                            liste.push(std::mem::take(&mut mevcut));
                        }
                        '(' => {
                            derinlik += 1;
                            mevcut.push(karakter);
                        }
                        ')' => {
                            derinlik = derinlik.saturating_sub(1);
                            mevcut.push(karakter);
                        }
                        _ => mevcut.push(karakter),
                    },
                }
            }
            if tirnak.is_some() {
                return Err(Hata::sablon(
                    dosya_adi,
                    satir,
                    "kapanmayan tirnak icinde arguman",
                ));
            }
            liste.push(mevcut);
            liste
                .into_iter()
                .map(|a| arguman_ayikla(dosya_adi, satir, a.trim()))
                .collect::<Result<Vec<_>, _>>()?
        };
        return Ok(Ifade::Cagri { islev, argumanlar });
    }
    if !metin
        .chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == '.' || c == '-')
    {
        return Err(Hata::sablon(
            dosya_adi,
            satir,
            format!("gecersiz degisken yolu `{metin}`"),
        ));
    }
    if let Ok(sayi) = metin.parse::<i64>() {
        return Ok(Ifade::Sayi(sayi));
    }
    Ok(Ifade::Yol(metin.to_string()))
}

/// Tek bir argümanı ayrıştırır.
fn arguman_ayikla(dosya_adi: &str, satir: usize, metin: &str) -> Result<Arguman, Hata> {
    if metin.len() >= 2
        && ((metin.starts_with('"') && metin.ends_with('"'))
            || (metin.starts_with('\'') && metin.ends_with('\'')))
    {
        return Ok(Arguman::Metin(metin[1..metin.len() - 1].to_string()));
    }
    if metin.is_empty() {
        return Err(Hata::sablon(dosya_adi, satir, "bos arguman"));
    }
    Ok(Arguman::Ifade(ifade_ayikla(dosya_adi, satir, metin)?))
}

/// `Ifade` değerini yalnızca hata mesajları için kullanılan metne çevirir.
#[allow(dead_code)]
fn ifade_metni(ifade: &Ifade) -> String {
    ifade.metin()
}
#[cfg(test)]
// unwrap/expect yalnizca testlerde ve gerekçeyle kullanilabilir (WORKER_CONTRACT.md § 4.2).
// Bir testte hata durumu testin kendisidir; sessizce yutulmamalidir.
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn baglam() -> Deger {
        Deger::harita(vec![
            (
                "site".to_string(),
                Deger::harita(vec![("baslik".to_string(), Deger::metin("StatikUsta"))]),
            ),
            (
                "sayfa".to_string(),
                Deger::harita(vec![
                    ("baslik".to_string(), Deger::metin("Yazi")),
                    ("taslak".to_string(), Deger::Yanlis),
                ]),
            ),
            (
                "etiketler".to_string(),
                Deger::liste(vec![Deger::metin("bir"), Deger::metin("iki")]),
            ),
        ])
    }

    #[test]
    fn duz_metni_oldugu_gibi_yazar() {
        let cikti = isle("s.html", "merhaba", &baglam()).unwrap();
        assert_eq!(cikti, "merhaba");
    }

    #[test]
    fn degisken_yerlestirir() {
        let cikti = isle("s.html", "<h1>{{ site.baslik }}</h1>", &baglam()).unwrap();
        assert_eq!(cikti, "<h1>StatikUsta</h1>");
    }

    #[test]
    fn varsayilan_cikti_kacislidir() {
        let baglam = Deger::harita(vec![(
            "x".to_string(),
            Deger::metin("<script>alert(1)</script>"),
        )]);
        let cikti = isle("s.html", "{{ x }}", &baglam).unwrap();
        assert_eq!(cikti, "&lt;script&gt;alert(1)&lt;/script&gt;");
        assert!(!cikti.contains("<script>"));
    }

    #[test]
    fn ham_suzgeci_kacisi_kaldirir() {
        let cikti = isle("s.html", "{{ site.baslik | ham }}", &baglam()).unwrap();
        assert_eq!(cikti, "StatikUsta");
    }

    #[test]
    fn ham_islevi_calisir() {
        let cikti = isle("s.html", "{{ ham(site.baslik) }}", &baglam()).unwrap();
        assert_eq!(cikti, "StatikUsta");
    }

    #[test]
    fn kosul_dogru_ysa_dogru_dali_calisir() {
        let cikti = isle(
            "s.html",
            "{% if site.baslik %}var{% else %}yok{% endif %}",
            &baglam(),
        )
        .unwrap();
        assert_eq!(cikti, "var");
    }

    #[test]
    fn kosul_yanlis_ysa_alternatif_calisir() {
        let cikti = isle(
            "s.html",
            "{% if sayfa.taslak %}t{% else %}y{% endif %}",
            &baglam(),
        )
        .unwrap();
        assert_eq!(cikti, "y");
    }

    #[test]
    fn dongu_dizi_gezer() {
        let cikti = isle(
            "s.html",
            "{% for e in etiketler %}[{{ e }}]{% endfor %}",
            &baglam(),
        )
        .unwrap();
        assert_eq!(cikti, "[bir][iki]");
    }

    #[test]
    fn ic_ice_dongu_calisir() {
        let sablon = "{% for a in etiketler %}{% for b in etiketler %}{{ a }}{{ b }} {% endfor %}{% endfor %}";
        let cikti = isle("s.html", sablon, &baglam()).unwrap();
        assert_eq!(cikti, "birbir biriki ikibir ikiiki ");
    }

    #[test]
    fn dongu_dongu_sinirini_asmaz() {
        let cok = Deger::harita(vec![(
            "xs".to_string(),
            Deger::liste((0..MAX_DONGU_ADIMI + 5).map(|i| Deger::Sayi(i as i64))),
        )]);
        let hata = isle("s.html", "{% for x in xs %}{{ x }}{% endfor %}", &cok).unwrap_err();
        assert!(hata.to_string().contains("dongu siniri"), "{hata}");
    }

    #[test]
    fn derinlik_siniri_korumali_donguyu_durdurur() {
        let cok = Deger::harita(vec![(
            "xs".to_string(),
            Deger::liste(vec![Deger::Sayi(1), Deger::Sayi(2)]),
        )]);
        let mut sablon = String::new();
        for _ in 0..(MAX_DERINLIK + 10) {
            sablon.push_str("{% for x in xs %}");
        }
        sablon.push_str("{{ x }}");
        for _ in 0..(MAX_DERINLIK + 10) {
            sablon.push_str("{% endfor %}");
        }
        let hata = isle("s.html", &sablon, &cok).unwrap_err();
        assert!(hata.to_string().contains("siniri"), "{hata}");
    }

    #[test]
    fn tanimsiz_degisken_hata_verir() {
        let hata = isle("s.html", "{{ olmayan }}", &baglam()).unwrap_err();
        assert!(hata.to_string().contains("tanimsiz degisken"), "{hata}");
        assert!(hata.to_string().contains("s.html"), "{hata}");
    }

    #[test]
    fn kapanmayan_isaret_hata_verir() {
        let hata = isle("s.html", "{{ site.baslik", &baglam()).unwrap_err();
        assert!(hata.to_string().contains("kapatilmadi"), "{hata}");
    }

    #[test]
    fn eslesmeyen_kapanis_hata_verir() {
        let hata = isle("s.html", "{% endif %}", &baglam()).unwrap_err();
        assert!(hata.to_string().contains("acilis yok"), "{hata}");
    }

    #[test]
    fn kapanmamis_blok_hata_verir() {
        let hata = isle("s.html", "{% if site.baslik %}a", &baglam()).unwrap_err();
        assert!(hata.to_string().contains("kapatilmadi"), "{hata}");
    }

    #[test]
    fn bilinmeyen_islev_hata_verir() {
        let hata = isle("s.html", "{{ yok(site.baslik) }}", &baglam()).unwrap_err();
        assert!(hata.to_string().contains("bilinmeyen islev"), "{hata}");
    }

    #[test]
    fn bilinmeyen_suzgec_hata_verir() {
        let hata = isle("s.html", "{{ site.baslik | ust }}", &baglam()).unwrap_err();
        assert!(hata.to_string().contains("bilinmeyen suzgec"), "{hata}");
    }

    #[test]
    fn kisalt_islevi_kirpar() {
        let cikti = isle("s.html", "{{ kisalt(site.baslik, 5) }}", &baglam()).unwrap();
        assert_eq!(cikti, "Stat...");
    }

    #[test]
    fn varlik_islevi_haritada_bulursa_imzali_adresi_verir() {
        let baglam = Deger::harita(vec![(
            "site".to_string(),
            Deger::harita(vec![(
                "varliklar".to_string(),
                Deger::harita(vec![(
                    "logo.png".to_string(),
                    Deger::metin("/static/logo.aabb.png"),
                )]),
            )]),
        )]);
        let cikti = isle("s.html", "{{ varlik(\"logo.png\") }}", &baglam).unwrap();
        assert_eq!(cikti, "/static/logo.aabb.png");
    }

    #[test]
    fn varlik_islevi_bilinmeyende_yolu_dondurur() {
        let cikti = isle("s.html", "{{ varlik(\"yok.png\") }}", &baglam()).unwrap();
        assert_eq!(cikti, "/static/yok.png");
    }

    #[test]
    fn gecersiz_yol_hata_verir() {
        let hata = isle("s.html", "{{ a b }}", &baglam()).unwrap_err();
        assert!(hata.to_string().contains("gecersiz"), "{hata}");
    }

    #[test]
    fn dogru_mu_kurallarini_uygular() {
        assert!(!Deger::Bos.dogru_mu());
        assert!(!Deger::Metin(String::new()).dogru_mu());
        assert!(!Deger::Sayi(0).dogru_mu());
        assert!(!Deger::Liste(vec![]).dogru_mu());
        assert!(Deger::Sayi(1).dogru_mu());
        assert!(Deger::Dogru.dogru_mu());
    }

    #[test]
    fn kacis_fonksiyonu_bes_karakteri_kacislar() {
        assert_eq!(
            kacis("<a href=\"x\">&'"),
            "&lt;a href=&quot;x&quot;&gt;&amp;&#39;"
        );
    }

    #[test]
    fn liste_yolu_indeksle_erisir() {
        let deger = Deger::liste(vec![Deger::metin("a"), Deger::metin("b")]);
        assert_eq!(deger.ara("1"), Some(&Deger::metin("b")));
        assert_eq!(deger.ara("5"), None);
    }

    #[test]
    fn hata_mesaji_dosya_ve_satir_tasarir() {
        let hata = SablonHatasi::TanimsizDegisken {
            satir: 9,
            yol: "x".to_string(),
        };
        assert_eq!(hata.to_string(), "satir 9: tanimsiz degisken `x`");
    }

    #[test]
    fn sablon_yeniden_kullanilabilir() {
        let sablon = Sablon::ayikla("s.html", "{{ site.baslik }}").unwrap();
        assert_eq!(sablon.calistir(&baglam()).unwrap(), "StatikUsta");
        assert_eq!(sablon.calistir(&baglam()).unwrap(), "StatikUsta");
    }
}
