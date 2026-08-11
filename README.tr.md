<a id="barepdf"></a>
<div align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="./assets/banner-dark.png">
    <source media="(prefers-color-scheme: light)" srcset="./assets/banner-white.png">
    <img src="./assets/banner-white.png" alt="BarePDF — Sade, hızlı, senin" width="100%">
  </picture>

  <h1>BarePDF</h1>
  <p><strong>Windows için hızlı ve özel PDF okuma.</strong></p>

  [![Son sürüm](https://img.shields.io/github/v/release/Woffluon/BarePDF?display_name=tag&style=flat-square&color=f7931e)](https://github.com/Woffluon/BarePDF/releases/latest)
  [![CI](https://img.shields.io/github/actions/workflow/status/Woffluon/BarePDF/ci.yml?branch=main&style=flat-square&label=CI)](https://github.com/Woffluon/BarePDF/actions/workflows/ci.yml)
  [![Belgeler](https://img.shields.io/badge/docs-online-0969da?style=flat-square)](https://woffluon.github.io/BarePDF/docs/)
  [![Windows](https://img.shields.io/badge/Windows-10%20%7C%2011-0078d4?style=flat-square&logo=windows11&logoColor=white)](#sistem-gereksinimleri)
  [![Lisans: MIT](https://img.shields.io/badge/license-MIT-2ea44f?style=flat-square)](./LICENSE)

  [İndir](https://woffluon.github.io/BarePDF/download/) ·
  [Belgeler](https://woffluon.github.io/BarePDF/docs/) ·
  [Değişiklik günlüğü](https://woffluon.github.io/BarePDF/changelog/) ·
  [Hata bildir](https://github.com/Woffluon/BarePDF/issues/new) ·
  [Katkıda bulun](#katkida-bulunma)

  [English](README.md) · [Türkçe](README.tr.md)
</div>

---

BarePDF; Windows 10 ve 11 için Rust, Slint ve PDFium ile geliştirilen açık kaynaklı bir PDF okuyucudur. Belge işlemlerini yerel tutar, başlangıçta telemetri veya hesap gerektirmez ve belge uzunluğuyla birlikte belleğin kontrolsüz büyümemesi için talep odaklı işleme ile sınırlandırılmış önbellekler kullanır.

<a id="icerik"></a>
## İçindekiler

- [BarePDF neden?](#neden-barepdf)
- [İndirme](#indirme)
- [Hızlı başlangıç](#hizli-baslangic)
- [Özellikler](#ozellikler)
- [Sistem gereksinimleri](#sistem-gereksinimleri)
- [Güncellemeler ve sürüm güvenliği](#guncellemeler-ve-surum-guvenligi)
- [Klavye kısayolları](#klavye-kisayollari)
- [Geliştirici rehberi](#gelistirici-rehberi)
- [Mimari](#mimari)
- [Test](#test)
- [Paketleme ve sürümler](#paketleme-ve-surumler)
- [Katkıda bulunma](#katkida-bulunma)
- [Gizlilik, güvenlik ve lisans](#gizlilik-guvenlik-ve-lisans)

<a id="neden-barepdf"></a>
## BarePDF neden?

| İlke | Anlamı |
| --- | --- |
| **Tasarımla hızlı** | Görünen sayfalara öncelik verilir; eski rasterleştirme işleri başlamadan iptal edilir. |
| **Sınırlandırılmış kaynaklar** | Byte bütçeli LRU önbellekleri ve talep odaklı küçük resimler, bitmap'lerin sınırsız büyümesini önler. |
| **Çevrimdışı öncelik** | PDF okumak için hesap, bulut hizmeti, analitik veya telemetri gerekmez. |
| **Yerel Windows entegrasyonu** | Kurulum kaydı, “Birlikte aç”, dosya bırakma, pano erişimi, yüksek DPI davranışı ve Varsayılan Uygulamalar akışı Windows kurallarını kullanır. |
| **Odaklı arayüz** | Okuma kontrolleri belge tuvalini kapatmadan erişilebilir kalır. |
| **Denetlenebilir sürümler** | Güncelleme meta verileri Ed25519 ile imzalanır; indirilen kurulum dosyaları URL, boyut, SHA-256 ve gömülü sürüm açısından kontrol edilir. |

<a id="indirme"></a>
## İndirme

[Resmi indirme sayfasını](https://woffluon.github.io/BarePDF/download/) kullanın. Sayfa, en güncel kararlı GitHub Release sürümünü okur ve tek bir kanonik kurulum dosyası, tek bir taşınabilir arşiv ve tek bir checksum bildirimi sunar.

| İstediğim | Seçim | Notlar |
| --- | --- | --- |
| BarePDF'i normal kurmak | `BarePDF-Setup-x64-vX.Y.Z.exe` | Önerilir. Yönetici hakkı gerektirmeden kullanıcı başına kurulum yapar. |
| Kurulum olmadan çalıştırmak | `BarePDF-Portable-x64-vX.Y.Z.zip` | Herhangi bir yere çıkarın, sonra `BarePDF.exe` dosyasını çalıştırın. Kurulum kayıt defteri değişikliği yapmaz. |
| İndirmeyi doğrulamak | `BarePDF-vX.Y.Z-SHA256SUMS.txt` | Kurulum ve taşınabilir paket için SHA-256 değerlerini içerir. |

> [!IMPORTANT]
> Kurulum dosyası kasıtlı olarak Authenticode ile imzalanmaz. Windows **Bilinmeyen yayımcı** uyarısı gösterebilir. Yalnızca resmi BarePDF sitesinden veya [`Woffluon/BarePDF` sürümlerinden](https://github.com/Woffluon/BarePDF/releases/latest) indirin; isterseniz ardından checksum değerini doğrulayın.

### Diğer sürüm dosyaları

Her BarePDF sürümü tam olarak beş proje varlığı yayımlar:

| Dosya | Amacı | Normal kullanıcıların ihtiyacı var mı? |
| --- | --- | :---: |
| `BarePDF-Setup-x64-vX.Y.Z.exe` | Windows kurulum dosyası | Evet |
| `BarePDF-Portable-x64-vX.Y.Z.zip` | Taşınabilir uygulama | İsteğe bağlı |
| `BarePDF-vX.Y.Z-SHA256SUMS.txt` | Kurulum ve taşınabilir paket checksum değerleri | İsteğe bağlı |
| `latest.json` | İmzalı güncelleyici meta verileri | Hayır |
| `latest.json.sig` | `latest.json` için Ed25519 imzası | Hayır |

GitHub ayrıca **Source code (zip)** ve **Source code (tar.gz)** dosyalarını otomatik ekler. Bu arşivler kaynak kodu içerir; çalıştırmaya hazır bir Windows uygulaması içermez.

### İndirmeyi doğrulama

~~~powershell
$Installer = Get-Item .\BarePDF-Setup-x64-v*.exe
Get-FileHash -Algorithm SHA256 -LiteralPath $Installer.FullName
~~~

Sonucu aynı sürümdeki `BarePDF-vX.Y.Z-SHA256SUMS.txt` dosyasındaki eşleşen girişle karşılaştırın.

<a id="hizli-baslangic"></a>
## Hızlı başlangıç

### Kurulum dosyası

1. [İndirme sayfasını](https://woffluon.github.io/BarePDF/download/) açın.
2. Tek `BarePDF-Setup-x64-vX.Y.Z.exe` dosyasını indirin.
3. Dosyayı çalıştırın ve kullanıcı başına kurulumu tamamlayın.
4. İsterseniz kurulumun Windows **Varsayılan Uygulamalar** ayarlarını açmasına izin verin, ardından `.pdf` dosyaları için BarePDF'i seçin.
5. `Ctrl+O` ile, dosyayı pencereye sürükleyerek veya Dosya Gezgini'nde çift tıklayarak PDF açın.

Varsayılan kurulum konumu:

~~~text
%LOCALAPPDATA%\Programs\BarePDF
~~~

### Taşınabilir sürüm

1. Aynı sayfadan `BarePDF-Portable-x64-vX.Y.Z.zip` dosyasını indirin.
2. Arşivi yazılabilir bir klasöre veya USB sürücüsüne çıkarın.
3. `BarePDF.exe` dosyasını çalıştırın.

<a id="ozellikler"></a>
## Özellikler

### Okuma ve gezinme

- Sürekli dikey ve tek sayfa görüntüleme modları.
- Sınırlandırılmış giriş doğrulamasıyla sayfa numarası üzerinden gezinme.
- Genişliğe sığdırma, sayfaya sığdırma, özel yakınlaştırma ve klavye yakınlaştırma kontrolleri.
- Tam ekran (`F11`) ve sunum (`F5`) modları.
- Sayfa küçük resimleri ve hiyerarşik belge ana hattında gezinme.
- Şifreli PDF'ler için parola istemi.
- Dosyaları hızlı yeniden açmak için son dosyalar listesi.
- İletişim kutusu, komut satırı, Dosya Gezgini ve sürükle-bırak ile dosya açma.

### Metin ve arayüz

- PDFium glif geometrisiyle desteklenen fareyle metin seçimi.
- Çift tıklamayla sözcük seçimi, üç tıklamayla satır seçimi ve `Ctrl+C` ile panoya kopyalama.
- Sistem, açık ve koyu temalar.
- İngilizce, Türkçe ve sistem dili modları.
- Yoğun ve farklı ölçekli ekranlar için yüksek DPI rasterleştirme.
- Duyarlı araç çubuğu ve daraltılabilir kenar çubuğu.

### Rasterleştirme davranışı

- İzole bir PDFium aktörü belge erişiminin sahibidir.
- Yüksek ve düşük öncelikli işleme kuyrukları görünür sayfaların akıcı kalmasını sağlar.
- Yinelenen istekler birleştirilir.
- Nesil belirteçleri, gezinme veya belge değişiminden sonra eski işleri reddeder.
- Ham ve arayüz bitmap önbellekleri açık byte bütçeleri kullanır.
- Küçük resim boyutları sayfa en-boy oranını korur.

<a id="sistem-gereksinimleri"></a>
## Sistem gereksinimleri

| Gereksinim | Desteklenen yapılandırma |
| --- | --- |
| İşletim sistemi | Windows 10 veya Windows 11 |
| Mimari | 64 bit x86 (`x86_64`) |
| Bellek | En az 512 MB; önerilen 1 GB |
| Depolama | Kurulu dosyalar için yaklaşık 50 MB |
| Ağ | Okuma için gerekmez; güncelleme denetimleri için isteğe bağlı |

<a id="guncellemeler-ve-surum-guvenligi"></a>
## Güncellemeler ve sürüm güvenliği

BarePDF, kullanıcı güncelleme denetimlerini etkinleştirip etkinleştirmeyeceğini seçene kadar çevrimdışı kalır. Etkinleştirildiğinde:

1. Uygulama en fazla 24 saatte bir denetim yapar.
2. `latest.json` ve `latest.json.sig` dosyalarını resmi GitHub Release uç noktasından indirir.
3. Manifestoyu uygulamaya sabitlenmiş Ed25519 açık anahtarıyla doğrular.
4. Kurulu sürümler arka planda daha yeni bir kurulum dosyası indirir.
5. Kurulum önerilmeden önce BarePDF tam sürüm URL'sini, dosya boyutunu, SHA-256 değerini ve gömülü Windows dosya sürümünü doğrular.
6. Kurulum yalnızca kullanıcının açık eyleminden sonra başlar. Taşınabilir sürümler kendilerini değiştirmek yerine sürüm sayfasına bağlantı verir.

Özel imzalama anahtarı yoksa veya sabitlenmiş açık anahtarla eşleşmiyorsa sürüm iş akışı güvenli biçimde başarısız olur. Geçersiz imzalar, güvenilmeyen yönlendirmeler, kısmi indirmeler, aynı sürümün yeniden kurulması ve sürüm düşürme reddedilir.

> [!NOTE]
> Manifesto imzalama, Authenticode sertifikası olmadan BarePDF'in güncelleme kanalını korur. Windows'un **Bilinmeyen yayımcı** istemini ortadan kaldırmaz.

<a id="klavye-kisayollari"></a>
## Klavye kısayolları

| Eylem | Kısayol |
| --- | --- |
| Belge aç | `Ctrl+O` |
| Önceki sayfa | `PageUp` veya `←` |
| Sonraki sayfa | `PageDown` veya `→` |
| Yakınlaştır | `+` veya `Ctrl++` |
| Uzaklaştır | `-` veya `Ctrl+-` |
| Seçili metni kopyala | `Ctrl+C` |
| Tam ekran | `F11` |
| Sunum modu | `F5` |
| Tam ekran veya sunumdan çık | `Esc` |
| Parolayı gönder | Parola iletişim kutusunda `Enter` |

Tam başvuru: [Klavye kısayolları belgeleri](https://woffluon.github.io/BarePDF/docs/user/keyboard-shortcuts/).

<a id="gelistirici-rehberi"></a>
## Geliştirici rehberi

### Ön koşullar

- x64 üzerinde Windows 10 veya 11.
- [Rust](https://www.rust-lang.org/tools/install) 1.92 veya daha yeni sürüm ve Cargo.
- Windows SDK ve C++ araçlarıyla Visual Studio 2022 Build Tools.
- Site için [Node.js](https://nodejs.org/) 22.12 veya daha yeni sürüm ve pnpm 10.
- Yalnızca kurulum dosyası oluştururken [Inno Setup](https://jrsoftware.org/isinfo.php).

### Masaüstü uygulamasını klonlama ve çalıştırma

~~~powershell
git clone https://github.com/Woffluon/BarePDF.git
cd BarePDF

# Sabitlenmiş PDFium derlemesini indirin ve depo checksum değerini doğrulayın.
powershell -File packaging/windows/scripts/fetch-pdfium.ps1 `
  -Destination target/debug/pdfium.dll

cargo run --package barepdf
~~~

BarePDF, `pdfium.dll` dosyasını uygulama çalıştırılabilir dosyasının yanında arar. Fetch script'i HTTPS üzerinden sabitlenmiş bir arşiv indirir ve checksum uyuşmazlığını reddeder.

### Web sitesini çalıştırma

~~~powershell
pnpm --dir website install --frozen-lockfile
pnpm --dir website run dev
~~~

Site kaynak kodu [`website/`](./website) içindedir. Üretim sayfaları, GitHub Pages üzerinden yayımlanan statik Astro çıktısıdır.

<a id="mimari"></a>
## Mimari

~~~mermaid
flowchart TD
    APP["apps/barepdf<br/>süreç + olay bağlantıları"] --> UI["barepdf-ui<br/>Slint sunumu"]
    APP --> CORE["barepdf-core<br/>tipler, yerleşim, tercihler"]
    APP --> PDF["barepdf-pdf<br/>PDFium adaptörü"]
    APP --> RENDER["barepdf-render<br/>zamanlayıcı + bitmap önbelleği"]
    APP --> PLATFORM["barepdf-platform<br/>işletim sistemi sözleşmeleri"]
    PLATFORM --> WIN["barepdf-platform-windows<br/>Win32 entegrasyonu"]
    APP --> I18N["barepdf-i18n<br/>İngilizce + Türkçe"]
    THUMB["barepdf-thumbnail<br/>Explorer küçük resimleri"] --> PDFIUM["yan pdfium.dll"]
    PDF --> PDFIUM
~~~

| Yol | Sorumluluk |
| --- | --- |
| [`apps/barepdf`](./apps/barepdf) | Çalıştırılabilir giriş noktası, tercih yükleme, güncelleme orkestrasyonu, olay döngüsü bağlantısı |
| [`crates/barepdf-core`](./crates/barepdf-core) | Motor bağımsız tipler, yerleşim hesapları, seçim, tercihler |
| [`crates/barepdf-pdf`](./crates/barepdf-pdf) | PDF trait'leri ve PDFium destekli belge uygulaması |
| [`crates/barepdf-render`](./crates/barepdf-render) | Öncelik zamanlayıcısı, iptal, tekilleştirme, bitmap önbellekleri |
| [`crates/barepdf-ui`](./crates/barepdf-ui) | Slint bileşenleri, araç çubuğu, belge tuvali, kenar çubuğu, iletişim kutuları |
| [`crates/barepdf-platform`](./crates/barepdf-platform) | Platform hizmet sözleşmeleri |
| [`crates/barepdf-platform-windows`](./crates/barepdf-platform-windows) | Windows iletişim kutuları, pano, dosya bırakma, kayıt defteri yardımcıları, güncelleme doğrulaması |
| [`crates/barepdf-i18n`](./crates/barepdf-i18n) | Dil seçimi ve eksiksiz çeviri tabloları |
| [`crates/barepdf-thumbnail`](./crates/barepdf-thumbnail) | Windows Explorer küçük resim sağlayıcısı |
| [`packaging/windows`](./packaging/windows) | Inno Setup tanımı ve deterministik paketleme script'leri |
| [`website`](./website) | Astro sitesi, kullanıcı belgeleri, geliştirici belgeleri, sürüm verisi entegrasyonu |

<a id="test"></a>
## Test

Depo kökünden sürüm öncesi doğrulamanın tamamını çalıştırın:

~~~powershell
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
cargo audit --deny warnings

pnpm --dir website run test
pnpm --dir website exec astro check
pnpm --dir website run build
~~~

CI ayrıca gerçek bir Windows kurulum dosyası oluşturup doğrular, sessiz kurulum/kaldırma davranışını çalıştırır, kayıt defteri yapılandırmasını kontrol eder ve kanonik sürüm varlık setini doğrular.

<a id="paketleme-ve-surumler"></a>
## Paketleme ve sürümler

Ürün sürümünün tek kaynağı [`Cargo.toml`](./Cargo.toml) içindeki `[workspace.package].version` alanıdır. Workspace crate'leri, arayüz meta verileri, kurulum meta verileri, dosya adları, etiketler, manifestolar ve site verileri buradan türetilir.

### Windows paketlerini yerelde oluşturma

~~~powershell
powershell -File packaging/windows/scripts/fetch-pdfium.ps1
powershell -File packaging/windows/scripts/stage-release.ps1
powershell -File packaging/windows/scripts/build-portable.ps1
powershell -File packaging/windows/scripts/build-installer.ps1
powershell -File packaging/windows/scripts/validate-installer.ps1
powershell -File packaging/windows/scripts/generate-checksums.ps1
powershell -File packaging/windows/scripts/test-release-manifest.ps1
~~~

Son imzasız varlıklar `target/release/artifacts/` içine yazılır. GitHub Actions, yayımlamadan önce `latest.json.sig` dosyasını ekler.

### Sürüm politikası

BarePDF, SemVer değişikliklerini belirlemek için Conventional Commits kullanır:

| Commit | Sürüm etkisi |
| --- | --- |
| `feat!:` veya `BREAKING CHANGE:` | Major |
| `feat:` | Minor |
| `fix:`, `perf:`, `refactor:`, `build:`, `security:` | Patch |
| `docs:`, `ci:`, `test:`, `chore:` | Ürün sürümünde değişiklik yok |

Commit oluşturmadan önce hazırlık ve doğrulama için tam commit mesajının aynısını kullanın:

~~~powershell
$CommitMessage = "fix(scope): describe the change"
powershell -File scripts/prepare-version.ps1 -Message $CommitMessage
powershell -File packaging/windows/scripts/validate-version.ps1 -Message $CommitMessage
~~~

`main` CI başarıyla tamamlandıktan sonra sürüm keşfi en yeni yayımlanmamış sürümü yayımlar, güncelleme meta verilerini imzalar, sürümü latest olarak işaretler ve GitHub Pages'i açıkça yeniler. Eski sürümler değiştirilemez kalır.

Daha fazla bilgi: [Paketleme belgeleri](https://woffluon.github.io/BarePDF/docs/developer/packaging/) ve [temiz Windows sürüm kontrol listesi](./docs/RELEASING.md).

<a id="katkida-bulunma"></a>
## Katkıda bulunma

1. [`AGENTS.md`](./AGENTS.md) dosyasını ve [geliştirici belgelerini](https://woffluon.github.io/BarePDF/docs/developer/) okuyun.
2. `main` dalından odaklı bir branch oluşturun.
3. Değişiklikleri cerrahi tutun ve önemsiz olmayan davranışı kanıtlayan en küçük regression testini ekleyin.
4. [Test](#test) bölümündeki ilgili kontrolleri çalıştırın.
5. Geçerli bir Conventional Commit mesajı kullanın.
6. Sorun, çözüm ve doğrulama notlarıyla bir pull request açın.

Sorunlar ve odaklı pull request'ler memnuniyetle karşılanır:

- [Hata bildir](https://github.com/Woffluon/BarePDF/issues/new)
- [Açık sorunlara göz at](https://github.com/Woffluon/BarePDF/issues)
- [Açık pull request'lere göz at](https://github.com/Woffluon/BarePDF/pulls)

<a id="gizlilik-guvenlik-ve-lisans"></a>
## Gizlilik, güvenlik ve lisans

- Telemetri, analitik, reklam, yapay zekâ hizmeti veya kullanıcı hesabı yoktur.
- PDF okuma çevrimdışı çalışır.
- Kullanıcı izin verene kadar güncelleme trafiği devre dışıdır.
- Güvenlik açısından hassas sürüm doğrulaması güvenli biçimde başarısız olur.
- Yerel PDFium paketleri sabitlenir ve hazırlama öncesinde checksum ile doğrulanır.

Şüpheli güvenlik açıklarını herkese açık bir issue yerine [GitHub Security Advisories](https://github.com/Woffluon/BarePDF/security/advisories/new) üzerinden özel olarak bildirin.

BarePDF [MIT Lisansı](./LICENSE) ile dağıtılır. Üçüncü taraf bileşenler ve bildirimler [`THIRD_PARTY_NOTICES.md`](./THIRD_PARTY_NOTICES.md) içinde belgelenmiştir.

---

<div align="center">
  <strong>Sade. Hızlı. Senin.</strong><br>
  <a href="#barepdf">Başa dön ↑</a>
</div>
